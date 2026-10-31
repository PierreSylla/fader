use crate::engine::{Adjust, Engine, Target};
use crate::screens::Screen;
use crate::{Error, Request, hypr, socket_path};
use gtk::prelude::*;
use gtk_layer_shell::{KeyboardMode, Layer, LayerShell};
use serde::Serialize;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const PANEL_INITIAL_WIDTH: i32 = 460;
const PANEL_INITIAL_HEIGHT: i32 = 32;
const REREAD_EVERY: Duration = Duration::from_secs(60);
const HOTPLUG_SETTLE: Duration = Duration::from_millis(500);
const RECONNECT_AFTER: Duration = Duration::from_secs(1);

pub fn run() -> Result<(), Error> {
    gtk::init().map_err(|_| Error::NoDisplay)?;
    let listener = bind_socket(&socket_path()?)?;

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_screens,
            set_brightness,
            panel_is_open,
            hide_panel,
            resize_panel
        ])
        .setup(move |app| {
            if !gtk_layer_shell::is_supported() {
                return Err(Error::NoLayerShell.into());
            }
            let panel = create_panel(app)?;
            let notified = panel.clone();
            let engine = Arc::new(Engine::new(move || notify_screens_changed(&notified)));
            app.manage(Arc::clone(&engine));
            let loading = (Arc::clone(&engine), panel.clone());
            std::thread::spawn(move || {
                refresh(&loading.0, &loading.1);
                watch_monitors(Arc::clone(&loading.0), loading.1.clone());
                reread_periodically(Arc::clone(&loading.0), loading.1.clone());
            });
            std::thread::spawn(move || serve(&listener, &panel, &engine));
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}

#[derive(Serialize)]
struct ScreenList {
    ready: bool,
    screens: Vec<Screen>,
}

#[tauri::command]
fn list_screens(engine: State<'_, Arc<Engine>>) -> ScreenList {
    ScreenList {
        ready: engine.is_ready(),
        screens: engine.screens(),
    }
}

#[tauri::command]
fn set_brightness(engine: State<'_, Arc<Engine>>, id: String, value: u8) -> Result<(), String> {
    engine
        .set(&Target::Screen(id), Adjust::Absolute(value.min(100)))
        .map(drop)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn panel_is_open(window: WebviewWindow) -> bool {
    window
        .gtk_window()
        .is_ok_and(|gtk| gtk.keyboard_mode() != KeyboardMode::None)
}

#[tauri::command]
fn hide_panel(window: WebviewWindow) {
    if let Ok(gtk) = window.gtk_window() {
        close_panel(&window, &gtk);
    }
}

#[tauri::command]
fn resize_panel(window: WebviewWindow, width: i32, height: i32) {
    if let Ok(gtk) = window.gtk_window() {
        gtk.set_size_request(width, height);
        gtk.resize(width, height);
    }
}

fn notify_screens_changed(panel: &WebviewWindow) {
    let _ = panel.emit("screens-changed", ());
}

fn refresh(engine: &Engine, panel: &WebviewWindow) {
    if let Err(e) = engine.refresh(None) {
        eprintln!("fader: {e}");
    }
    notify_screens_changed(panel);
}

fn watch_monitors(engine: Arc<Engine>, panel: WebviewWindow) {
    std::thread::spawn(move || {
        loop {
            let result = hypr::watch_monitors(|| {
                std::thread::sleep(HOTPLUG_SETTLE);
                refresh(&engine, &panel);
            });
            if let Err(e) = result {
                eprintln!("fader: {e}");
            }
            std::thread::sleep(RECONNECT_AFTER);
        }
    });
}

fn reread_periodically(engine: Arc<Engine>, panel: WebviewWindow) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(REREAD_EVERY);
            engine.reread();
            notify_screens_changed(&panel);
        }
    });
}

/// SPEC §5: nothing is written outside `~/.config/fader` and `~/.local/state/fader`.
fn webview_dir(state_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let state = state_home
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(home?).join(".local/state")))?;
    Some(state.join("fader/webview"))
}

fn bind_socket(path: &Path) -> Result<UnixListener, Error> {
    if UnixStream::connect(path).is_ok() {
        return Err(Error::AlreadyRunning);
    }
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    Ok(UnixListener::bind(path)?)
}

fn serve(listener: &UnixListener, panel: &WebviewWindow, engine: &Arc<Engine>) {
    for stream in listener.incoming().flatten() {
        let panel = panel.clone();
        let engine = Arc::clone(engine);
        std::thread::spawn(move || handle_client(stream, &panel, &engine));
    }
}

fn handle_client(stream: UnixStream, panel: &WebviewWindow, engine: &Engine) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let words: Vec<&str> = line.split_whitespace().collect();
    let result = match Request::parse(&words) {
        Some(Request::Panel) => {
            let panel_on_main = panel.clone();
            panel
                .run_on_main_thread(move || toggle_panel(&panel_on_main))
                .map(|()| String::new())
                .map_err(Error::from)
        }
        Some(request) => {
            engine.wait_ready();
            request.execute(engine)
        }
        None => Err(Error::BadRequest(line.trim().to_owned())),
    };
    let reply = match result {
        Ok(body) => format!("ok\n{body}"),
        Err(e) => format!("err\n{e}"),
    };
    let _ = reader.into_inner().write_all(reply.as_bytes());
}

fn create_panel(app: &tauri::App) -> Result<WebviewWindow, Error> {
    let data_dir = webview_dir(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
        .ok_or(Error::NoHome)?;
    let panel = WebviewWindowBuilder::new(app, "panel", WebviewUrl::default())
        .title("Fader")
        .data_directory(data_dir)
        .visible(false)
        .decorations(false)
        .transparent(true)
        .build()?;
    let gtk = panel.gtk_window()?;
    gtk.init_layer_shell();
    gtk.set_namespace("fader-panel");
    gtk.set_layer(Layer::Overlay);
    gtk.set_size_request(PANEL_INITIAL_WIDTH, PANEL_INITIAL_HEIGHT);
    gtk.connect_focus_in_event(|window, _| {
        if window.keyboard_mode() == KeyboardMode::Exclusive {
            window.set_keyboard_mode(KeyboardMode::OnDemand);
        }
        gtk::glib::Propagation::Proceed
    });
    let on_focus_out = panel.clone();
    gtk.connect_focus_out_event(move |window, _| {
        if window.keyboard_mode() != KeyboardMode::None {
            close_panel(&on_focus_out, window);
        }
        gtk::glib::Propagation::Proceed
    });
    close_panel(&panel, &gtk);
    gtk.show_all();
    Ok(panel)
}

fn toggle_panel(panel: &WebviewWindow) {
    let Ok(gtk) = panel.gtk_window() else { return };
    if gtk.keyboard_mode() == KeyboardMode::None {
        follow_focused_monitor(&gtk);
        gtk.input_shape_combine_region(None);
        gtk.set_keyboard_mode(KeyboardMode::Exclusive);
        let _ = panel.emit("panel-visibility", true);
    } else {
        close_panel(panel, &gtk);
    }
}

fn close_panel(panel: &WebviewWindow, gtk: &gtk::ApplicationWindow) {
    gtk.input_shape_combine_region(Some(&gtk::cairo::Region::create()));
    gtk.set_keyboard_mode(KeyboardMode::None);
    let _ = panel.emit("panel-visibility", false);
}

/// GDK has no connector name: monitors are matched by position (ADR-0002).
fn follow_focused_monitor(gtk: &gtk::ApplicationWindow) {
    let Ok(monitors) = hypr::monitors() else {
        return;
    };
    let Some(focused) = monitors.iter().find(|m| m.focused) else {
        return;
    };
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let target = (0..display.n_monitors())
        .filter_map(|i| display.monitor(i))
        .find(|m| {
            let area = m.geometry();
            area.x() == focused.x && area.y() == focused.y
        });
    if let Some(target) = target
        && gtk.monitor().as_ref() != Some(&target)
    {
        gtk.set_monitor(&target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webview_data_stays_in_the_state_directory() {
        let dir = |state: Option<&str>, home: Option<&str>| {
            webview_dir(state.map(Into::into), home.map(Into::into))
        };

        assert_eq!(
            dir(None, Some("/home/a")),
            Some("/home/a/.local/state/fader/webview".into())
        );
        assert_eq!(
            dir(Some("/s"), Some("/home/a")),
            Some("/s/fader/webview".into())
        );
        assert_eq!(
            dir(Some(""), Some("/home/a")),
            Some("/home/a/.local/state/fader/webview".into())
        );
        assert_eq!(dir(None, None), None);
    }

    #[test]
    fn bind_socket_replaces_stale_file_and_refuses_live_daemon() {
        let path = std::env::temp_dir().join(format!("fader-test-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);

        std::fs::write(&path, "").unwrap();
        let live = bind_socket(&path).unwrap();
        assert!(matches!(bind_socket(&path), Err(Error::AlreadyRunning)));

        drop(live);
        assert!(bind_socket(&path).is_ok());
        std::fs::remove_file(&path).unwrap();
    }
}
