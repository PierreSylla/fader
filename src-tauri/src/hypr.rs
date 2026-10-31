//! IPC Hyprland (ADR-0004). Parle directement à `.socket.sock`, sans `hyprctl`.

use crate::Error;
use serde::Deserialize;
use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Deserialize)]
pub struct Monitor {
    pub name: String,
    pub make: String,
    pub model: String,
    pub disabled: bool,
}

impl Monitor {
    pub fn label(&self) -> String {
        format!("{} {}", self.make, self.model)
    }
}

pub fn monitors() -> Result<Vec<Monitor>, Error> {
    parse_monitors(&request("j/monitors all")?)
}

fn parse_monitors(reply: &str) -> Result<Vec<Monitor>, Error> {
    serde_json::from_str(reply).map_err(Error::HyprlandReply)
}

/// Une connexion par requête : d'après le wiki, un `.socket.sock` laissé ouvert gèle Hyprland.
fn request(command: &str) -> Result<String, Error> {
    let mut stream = UnixStream::connect(socket_path()?).map_err(|e| match e.kind() {
        ErrorKind::NotFound | ErrorKind::ConnectionRefused => Error::NoHyprland,
        _ => e.into(),
    })?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    stream.write_all(command.as_bytes())?;
    let mut reply = String::new();
    stream
        .read_to_string(&mut reply)
        .map_err(|e| match e.kind() {
            ErrorKind::WouldBlock | ErrorKind::TimedOut => Error::NoHyprland,
            _ => e.into(),
        })?;
    Ok(reply)
}

fn socket_path() -> Result<PathBuf, Error> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or(Error::NoRuntimeDir)?;
    let instance = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").ok_or(Error::NoHyprland)?;
    Ok(PathBuf::from(runtime)
        .join("hypr")
        .join(instance)
        .join(".socket.sock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_monitors_and_ignores_other_fields() {
        // Extrait réel de `hyprctl monitors all -j`, Hyprland 0.56.2.
        let reply = r#"[{"id": 0, "name": "DP-1", "description": "AOC Q27B35E 2S6S3HA016250",
            "make": "AOC", "model": "Q27B35E", "serial": "2S6S3HA016250", "width": 2560,
            "activeWorkspace": {"id": 4, "name": "4"}, "focused": true, "disabled": false},
            {"id": 1, "name": "DP-2", "make": "Samsung Electric Company", "model": "LC32G5xT",
            "disabled": true}]"#;

        let monitors = parse_monitors(reply).unwrap();

        assert_eq!(monitors.len(), 2);
        assert_eq!(monitors[0].name, "DP-1");
        assert_eq!(monitors[1].label(), "Samsung Electric Company LC32G5xT");
        assert!(!monitors[0].disabled);
        assert!(monitors[1].disabled);
    }

    #[test]
    fn rejects_a_reply_that_is_not_a_monitor_list() {
        assert!(matches!(
            parse_monitors("unknown request"),
            Err(Error::HyprlandReply(_))
        ));
    }
}
