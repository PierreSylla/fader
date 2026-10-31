use crate::engine::Engine;
use crate::{Error, Request, hypr, socket_path};
use gtk::prelude::*;
use gtk_layer_shell::{KeyboardMode, Layer, LayerShell};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::{WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const PANEL_WIDTH: i32 = 460;
const PANEL_HEIGHT: i32 = 300;
const REREAD_EVERY: Duration = Duration::from_secs(60);
const HOTPLUG_SETTLE: Duration = Duration::from_millis(500);
const RECONNECT_AFTER: Duration = Duration::from_secs(1);

pub fn run() -> Result<(), Error> {
    gtk::init().map_err(|_| Error::NoDisplay)?;
    let listener = bind_socket(&socket_path()?)?;

    tauri::Builder::default()
        .setup(move |app| {
            if !gtk_layer_shell::is_supported() {
                return Err(Error::NoLayerShell.into());
            }
            let panel = create_panel(app)?;
            std::thread::spawn(move || {
                let engine = Arc::new(Engine::default());
                refresh(&engine);
                watch_monitors(Arc::clone(&engine));
                reread_periodically(Arc::clone(&engine));
                serve(&listener, &panel, &engine);
            });
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}

fn refresh(engine: &Engine) {
    if let Err(e) = engine.refresh(None) {
        eprintln!("fader: {e}");
    }
}

fn watch_monitors(engine: Arc<Engine>) {
    std::thread::spawn(move || {
        loop {
            let result = hypr::watch_monitors(|| {
                std::thread::sleep(HOTPLUG_SETTLE);
                refresh(&engine);
            });
            if let Err(e) = result {
                eprintln!("fader: {e}");
            }
            std::thread::sleep(RECONNECT_AFTER);
        }
    });
}

fn reread_periodically(engine: Arc<Engine>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(REREAD_EVERY);
            engine.reread();
        }
    });
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
        Some(request) => request.execute(engine),
        None => Err(Error::BadRequest(line.trim().to_owned())),
    };
    let reply = match result {
        Ok(body) => format!("ok\n{body}"),
        Err(e) => format!("err\n{e}"),
    };
    let _ = reader.into_inner().write_all(reply.as_bytes());
}

fn create_panel(app: &tauri::App) -> Result<WebviewWindow, Error> {
    let panel = WebviewWindowBuilder::new(app, "panel", WebviewUrl::default())
        .title("Fader")
        .visible(false)
        .decorations(false)
        .transparent(true)
        .build()?;
    let gtk = panel.gtk_window()?;
    gtk.init_layer_shell();
    gtk.set_namespace("fader-panel");
    gtk.set_layer(Layer::Overlay);
    gtk.set_size_request(PANEL_WIDTH, PANEL_HEIGHT);
    gtk.connect_focus_in_event(|window, _| {
        if window.keyboard_mode() == KeyboardMode::Exclusive {
            window.set_keyboard_mode(KeyboardMode::OnDemand);
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
        gtk.input_shape_combine_region(None);
        gtk.set_keyboard_mode(KeyboardMode::Exclusive);
        let _ = panel.eval("document.documentElement.toggleAttribute('data-open', true)");
    } else {
        close_panel(panel, &gtk);
    }
}

fn close_panel(panel: &WebviewWindow, gtk: &gtk::ApplicationWindow) {
    gtk.input_shape_combine_region(Some(&gtk::cairo::Region::create()));
    gtk.set_keyboard_mode(KeyboardMode::None);
    let _ = panel.eval("document.documentElement.toggleAttribute('data-open', false)");
}

#[cfg(test)]
mod tests {
    use super::*;

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
