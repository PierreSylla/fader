use crate::Error;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Deserialize)]
pub struct Monitor {
    pub name: String,
    pub make: String,
    pub model: String,
    pub serial: String,
    pub focused: bool,
    pub disabled: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub transform: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Monitor {
    pub fn label(&self) -> String {
        format!("{} {}", self.make, self.model)
    }

    /// Odd transforms rotate the output by 90 or 270 degrees.
    pub fn geometry(&self) -> Geometry {
        let (width, height) = if self.transform % 2 == 1 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        };
        Geometry {
            x: self.x,
            y: self.y,
            width,
            height,
        }
    }
}

pub fn monitors() -> Result<Vec<Monitor>, Error> {
    parse_monitors(&request("j/monitors all")?)
}

pub fn focused() -> Result<String, Error> {
    monitors()?
        .into_iter()
        .find(|m| m.focused)
        .map(|m| m.name)
        .ok_or(Error::NoHyprland)
}

#[derive(Debug, Deserialize)]
struct Status {
    #[serde(rename = "configProvider")]
    config_provider: String,
}

#[derive(Debug, Deserialize)]
struct Version {
    version: String,
}

pub fn version() -> Result<String, Error> {
    let reply = request("j/version")?;
    let version: Version = serde_json::from_str(&reply).map_err(Error::HyprlandReply)?;
    Ok(version.version)
}

pub fn config_provider() -> Result<String, Error> {
    parse_config_provider(&request("j/status")?)
}

fn parse_config_provider(reply: &str) -> Result<String, Error> {
    if reply.trim() == "unknown request" {
        return Ok("hyprlang".to_owned());
    }
    let status: Status = serde_json::from_str(reply).map_err(Error::HyprlandReply)?;
    Ok(status.config_provider)
}

/// Calls `changed` on each monitor event; returns when the connection is lost.
pub fn watch_monitors(mut changed: impl FnMut()) -> Result<(), Error> {
    let stream = connect(".socket2.sock")?;
    for line in BufReader::new(stream).lines() {
        if is_monitor_event(&line?) {
            changed();
        }
    }
    Ok(())
}

fn is_monitor_event(line: &str) -> bool {
    line.starts_with("monitoraddedv2>>") || line.starts_with("monitorremovedv2>>")
}

fn parse_monitors(reply: &str) -> Result<Vec<Monitor>, Error> {
    serde_json::from_str(reply).map_err(Error::HyprlandReply)
}

/// One connection per request: an open `.socket.sock` freezes Hyprland (ADR-0004).
fn request(command: &str) -> Result<String, Error> {
    let mut stream = connect(".socket.sock")?;
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

fn connect(socket: &str) -> Result<UnixStream, Error> {
    UnixStream::connect(socket_path(socket)?).map_err(|e| match e.kind() {
        ErrorKind::NotFound | ErrorKind::ConnectionRefused => Error::NoHyprland,
        _ => e.into(),
    })
}

fn socket_path(socket: &str) -> Result<PathBuf, Error> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or(Error::NoRuntimeDir)?;
    let instance = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").ok_or(Error::NoHyprland)?;
    Ok(PathBuf::from(runtime)
        .join("hypr")
        .join(instance)
        .join(socket))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_monitors_and_ignores_other_fields() {
        // Real excerpt, Hyprland 0.56.2.
        let reply = r#"[{"id": 0, "name": "DP-1", "description": "AOC Q27B35E 2S6S3HA016250",
            "make": "AOC", "model": "Q27B35E", "serial": "2S6S3HA016250", "width": 2560,
            "height": 1440, "x": 5120, "y": 0, "transform": 0,
            "activeWorkspace": {"id": 4, "name": "4"}, "focused": true, "disabled": false},
            {"id": 1, "name": "DP-2", "make": "Samsung Electric Company", "model": "LC32G5xT",
            "serial": "", "focused": false, "disabled": true,
            "x": 0, "y": 0, "width": 1440, "height": 2560, "transform": 1}]"#;

        let monitors = parse_monitors(reply).unwrap();

        assert_eq!(monitors.len(), 2);
        assert_eq!(monitors[0].name, "DP-1");
        assert_eq!(monitors[1].label(), "Samsung Electric Company LC32G5xT");
        assert!(!monitors[0].disabled);
        assert!(monitors[1].disabled);
        assert_eq!(monitors[0].geometry().width, 2560);
        assert_eq!(monitors[1].geometry().width, 2560);
        assert_eq!(monitors[1].geometry().height, 1440);
    }

    #[test]
    fn detects_the_config_provider() {
        let lua = r#"{"configProvider": "lua", "backend": "drm"}"#;
        assert_eq!(parse_config_provider(lua).unwrap(), "lua");
        assert_eq!(
            parse_config_provider("unknown request").unwrap(),
            "hyprlang"
        );
        assert!(parse_config_provider("garbage").is_err());
    }

    #[test]
    fn recognizes_monitor_events_only() {
        assert!(is_monitor_event("monitoraddedv2>>1,DP-2,Samsung"));
        assert!(is_monitor_event("monitorremovedv2>>1,DP-2,Samsung"));
        assert!(!is_monitor_event("monitoradded>>DP-2"));
        assert!(!is_monitor_event("openlayer>>fader-panel"));
    }

    #[test]
    fn rejects_a_reply_that_is_not_a_monitor_list() {
        assert!(matches!(
            parse_monitors("unknown request"),
            Err(Error::HyprlandReply(_))
        ));
    }
}
