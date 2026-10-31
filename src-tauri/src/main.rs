use gtk::prelude::*;
use gtk_layer_shell::{KeyboardMode, Layer, LayerShell};
use std::io::{self, BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;
use tauri::{WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const PANEL_WIDTH: i32 = 460;
const PANEL_HEIGHT: i32 = 300;
const USAGE: &str = "usage: fader <daemon|panel>";

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("XDG_RUNTIME_DIR is not set")]
    NoRuntimeDir,
    #[error("fader daemon is already running")]
    AlreadyRunning,
    #[error("fader daemon is not running, start it with: fader daemon")]
    NotRunning,
    #[error("cannot connect to a Wayland display")]
    NoDisplay,
    #[error("this compositor does not support the layer-shell protocol")]
    NoLayerShell,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["daemon"] => daemon(),
        ["panel"] => send("panel"),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fader: {e}");
            ExitCode::FAILURE
        }
    }
}

fn socket_path() -> Result<PathBuf, Error> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").ok_or(Error::NoRuntimeDir)?;
    Ok(PathBuf::from(dir).join("fader.sock"))
}

fn send(command: &str) -> Result<(), Error> {
    let mut stream = UnixStream::connect(socket_path()?).map_err(|e| match e.kind() {
        ErrorKind::NotFound | ErrorKind::ConnectionRefused => Error::NotRunning,
        _ => e.into(),
    })?;
    writeln!(stream, "{command}")?;
    Ok(())
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

fn daemon() -> Result<(), Error> {
    gtk::init().map_err(|_| Error::NoDisplay)?;
    let listener = bind_socket(&socket_path()?)?;

    tauri::Builder::default()
        .setup(move |app| {
            if !gtk_layer_shell::is_supported() {
                return Err(Error::NoLayerShell.into());
            }
            let panel = create_panel(app)?;
            std::thread::spawn(move || serve(&listener, &panel));
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
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

fn serve(listener: &UnixListener, panel: &WebviewWindow) {
    for stream in listener.incoming().flatten() {
        // Un client muet ne doit pas bloquer les suivants.
        let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
        let mut line = String::new();
        if BufReader::new(stream).read_line(&mut line).is_ok() && line.trim() == "panel" {
            let panel_on_main = panel.clone();
            let _ = panel.run_on_main_thread(move || toggle_panel(&panel_on_main));
        }
    }
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
