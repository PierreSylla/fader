mod backlight;
mod daemon;
mod ddc;
mod doctor;
mod engine;
mod hypr;
mod i18n;
mod queue;
mod screens;
mod tool;

use engine::{Adjust, Engine, Target};
use i18n::{t, tf};
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

const CLIENT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}", t("XDG_RUNTIME_DIR is not set"))]
    NoRuntimeDir,
    #[error("{}", t("fader daemon is already running"))]
    AlreadyRunning,
    #[error("{}", t("fader daemon is not running, start it with: fader daemon"))]
    NotRunning,
    #[error("{}", t("cannot connect to a Wayland display"))]
    NoDisplay,
    #[error("{}", t("this compositor does not support the layer-shell protocol"))]
    NoLayerShell,
    #[error("{}", t("cannot reach Hyprland, is it running in this session?"))]
    NoHyprland,
    #[error("{}", tf("unexpected reply from Hyprland: {}", .0))]
    HyprlandReply(serde_json::Error),
    #[error("{}", tf("{} is not installed", .0))]
    ToolMissing(&'static str),
    #[error("{}", tf("{} did not answer in time", .0))]
    ToolTimeout(&'static str),
    #[error("{0}")]
    Tool(String),
    #[error("{}", tf("unknown screen: {}", .0))]
    UnknownScreen(String),
    #[error("{id}: {reason}")]
    ScreenUnavailable { id: String, reason: String },
    #[error("{0}")]
    SetFailed(String),
    #[error("{}", tf("invalid request: {}", .0))]
    BadRequest(String),
    #[error("{0}")]
    Daemon(String),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
}

#[derive(Debug, PartialEq)]
pub enum Request {
    Daemon,
    Doctor,
    Panel,
    List,
    Get(String),
    Set(Target, Adjust),
}

impl Request {
    pub fn parse(words: &[&str]) -> Option<Self> {
        Some(match *words {
            ["daemon"] => Request::Daemon,
            ["doctor"] => Request::Doctor,
            ["panel"] => Request::Panel,
            ["list"] => Request::List,
            ["get", screen] => Request::Get(screen.to_owned()),
            ["set", target, value] => {
                let target = match target {
                    "focused" => Target::Focused,
                    "all" => Target::All,
                    screen => Target::Screen(screen.to_owned()),
                };
                Request::Set(target, Adjust::parse(value)?)
            }
            _ => return None,
        })
    }

    pub fn execute(&self, engine: &Engine) -> Result<String, Error> {
        match self {
            Request::List => Ok(engine.list()),
            Request::Get(id) => engine.get(id),
            Request::Set(target, adjust) => engine.set(target, *adjust),
            _ => Err(Error::BadRequest(format!("{self:?}"))),
        }
    }

    fn only_screen(&self) -> Option<&str> {
        match self {
            Request::Get(id) | Request::Set(Target::Screen(id), _) => Some(id),
            _ => None,
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let Some(request) = Request::parse(&words) else {
        eprintln!("{}", t(i18n::USAGE));
        return ExitCode::from(2);
    };
    let result = match request {
        Request::Daemon => daemon::run().map(|()| String::new()),
        Request::Doctor => doctor::run(),
        Request::Panel => send(&words.join(" ")),
        request => match send(&words.join(" ")) {
            Err(Error::NotRunning) => direct(&request),
            reply => reply,
        },
    };
    let result = result.and_then(|body| Ok(io::stdout().write_all(body.as_bytes())?));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Io(e)) if e.kind() == ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fader: {e}");
            ExitCode::FAILURE
        }
    }
}

fn direct(request: &Request) -> Result<String, Error> {
    let engine = Engine::default();
    engine.refresh(request.only_screen())?;
    let output = request.execute(&engine)?;
    engine.finish_writes()?;
    Ok(output)
}

pub fn socket_path() -> Result<PathBuf, Error> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").ok_or(Error::NoRuntimeDir)?;
    Ok(PathBuf::from(dir).join("fader.sock"))
}

fn send(command: &str) -> Result<String, Error> {
    let mut stream = UnixStream::connect(socket_path()?).map_err(|e| match e.kind() {
        ErrorKind::NotFound | ErrorKind::ConnectionRefused => Error::NotRunning,
        _ => e.into(),
    })?;
    stream.set_read_timeout(Some(CLIENT_TIMEOUT))?;
    writeln!(stream, "{command}")?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply)?;
    parse_reply(&reply)
}

fn parse_reply(reply: &str) -> Result<String, Error> {
    match reply.split_once('\n') {
        Some(("ok", body)) => Ok(body.to_owned()),
        Some(("err", message)) => Err(Error::Daemon(message.to_owned())),
        _ => Err(Error::Daemon(reply.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_requests() {
        assert_eq!(Request::parse(&["list"]), Some(Request::List));
        assert_eq!(
            Request::parse(&["set", "focused", "+5"]),
            Some(Request::Set(Target::Focused, Adjust::Relative(5)))
        );
        assert_eq!(
            Request::parse(&["set", "all", "40"]),
            Some(Request::Set(Target::All, Adjust::Absolute(40)))
        );
        assert_eq!(
            Request::parse(&["set", "DP-1", "-5"]),
            Some(Request::Set(
                Target::Screen("DP-1".into()),
                Adjust::Relative(-5)
            ))
        );
        assert_eq!(Request::parse(&["set", "DP-1", "300"]), None);
        assert_eq!(Request::parse(&["set", "DP-1"]), None);
        assert_eq!(Request::parse(&[]), None);
    }

    #[test]
    fn parses_daemon_replies() {
        assert_eq!(parse_reply("ok\nDP-1\t50\n").unwrap(), "DP-1\t50\n");
        assert_eq!(parse_reply("ok\n").unwrap(), "");
        assert!(matches!(parse_reply("err\nnope"), Err(Error::Daemon(m)) if m == "nope"));
        assert!(matches!(parse_reply(""), Err(Error::Daemon(_))));
    }
}
