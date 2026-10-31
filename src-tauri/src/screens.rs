use crate::ddc::Brightness;
use crate::hypr::{Geometry, Monitor};
use crate::i18n::t;
use crate::{Error, backlight, ddc, hypr};
use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Backend {
    Backlight { device: String },
    Ddc { bus: u32 },
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Busy,
    Unreachable,
    Permission,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Screen {
    pub id: String,
    pub label: String,
    pub geometry: Geometry,
    pub backend: Backend,
    pub value: Option<u8>,
    /// Value the hardware still has while a write is in flight.
    pub applied: Option<u8>,
    pub max: Option<u32>,
    pub status: Status,
    pub reason: Option<String>,
}

impl Screen {
    pub fn writable(&self) -> bool {
        self.backend != Backend::None
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Backend::Backlight { .. } => "backlight",
            Backend::Ddc { .. } => "ddc",
            Backend::None => "none",
        })
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Status::Ok => "ok",
            Status::Busy => "busy",
            Status::Unreachable => "unreachable",
            Status::Permission => "permission",
        })
    }
}

pub fn read(only: Option<&str>) -> Result<Vec<Screen>, Error> {
    let monitors: Vec<Monitor> = hypr::monitors()?
        .into_iter()
        .filter(|m| only.is_none_or(|id| m.name == id))
        .collect();
    if let (Some(id), true) = (only, monitors.is_empty()) {
        return Err(Error::UnknownScreen(id.to_owned()));
    }
    let detected = if monitors.iter().all(|m| backlight::is_internal(&m.name)) {
        Ok(Vec::new())
    } else {
        ddc::detect()
    };
    Ok(std::thread::scope(|scope| {
        let readers: Vec<_> = monitors
            .iter()
            .map(|m| {
                let detected = &detected;
                scope.spawn(move || read_screen(m, detected))
            })
            .collect();
        readers
            .into_iter()
            .zip(&monitors)
            .map(|(reader, monitor)| {
                reader.join().unwrap_or_else(|_| {
                    let reason = t("internal error").to_owned();
                    unavailable(monitor, Backend::None, Status::Unreachable, reason)
                })
            })
            .collect()
    }))
}

pub fn read_value(backend: &Backend) -> Result<Brightness, Error> {
    match backend {
        Backend::Backlight { device } => backlight::get(device),
        Backend::Ddc { bus } => ddc::get(*bus),
        Backend::None => Err(Error::Tool(t("no brightness backend found").to_owned())),
    }
}

pub fn write(backend: &Backend, raw: u32) -> Result<(), Error> {
    match backend {
        Backend::Backlight { device } => backlight::set(device, raw),
        Backend::Ddc { bus } => ddc::set(*bus, raw),
        Backend::None => Err(Error::Tool(t("no brightness backend found").to_owned())),
    }
}

fn read_screen(monitor: &Monitor, detected: &Result<Vec<ddc::Display>, Error>) -> Screen {
    if monitor.disabled {
        let reason = t("output is disabled").to_owned();
        return unavailable(monitor, Backend::None, Status::Unreachable, reason);
    }
    let backend = match find_backend(monitor, detected) {
        Ok(backend) => backend,
        Err(screen) => return *screen,
    };
    match read_value(&backend) {
        Ok(brightness) => Screen {
            id: monitor.name.clone(),
            label: monitor.label(),
            geometry: monitor.geometry(),
            backend,
            value: Some(brightness.percent()),
            applied: None,
            max: Some(brightness.max),
            status: Status::Ok,
            reason: None,
        },
        Err(e) => unavailable(monitor, backend, status_of(&e.to_string()), e.to_string()),
    }
}

fn find_backend(
    monitor: &Monitor,
    detected: &Result<Vec<ddc::Display>, Error>,
) -> Result<Backend, Box<Screen>> {
    let none = |status, reason| Box::new(unavailable(monitor, Backend::None, status, reason));
    let no_backend = || t("no brightness backend found").to_owned();
    if backlight::is_internal(&monitor.name) {
        return backlight::find()
            .map(|device| Backend::Backlight { device })
            .ok_or_else(|| none(Status::Unreachable, no_backend()));
    }
    let displays = detected
        .as_ref()
        .map_err(|e| none(status_of(&e.to_string()), e.to_string()))?;
    let display =
        find_display(monitor, displays).ok_or_else(|| none(Status::Unreachable, no_backend()))?;
    let backend = Backend::Ddc { bus: display.bus };
    match &display.problem {
        Some(problem) => Err(Box::new(unavailable(
            monitor,
            backend,
            status_of(problem),
            problem.clone(),
        ))),
        None => Ok(backend),
    }
}

fn find_display<'a>(monitor: &Monitor, displays: &'a [ddc::Display]) -> Option<&'a ddc::Display> {
    displays
        .iter()
        .find(|d| d.connector.as_deref() == Some(monitor.name.as_str()))
        .or_else(|| {
            displays.iter().find(|d| {
                d.connector.is_none()
                    && !monitor.serial.is_empty()
                    && d.serial == monitor.serial
                    && d.model == monitor.model
            })
        })
}

pub fn status_of(message: &str) -> Status {
    if message.contains("i2c_permissions") || message.contains("Permission denied") {
        Status::Permission
    } else {
        Status::Unreachable
    }
}

fn unavailable(monitor: &Monitor, backend: Backend, status: Status, reason: String) -> Screen {
    Screen {
        id: monitor.name.clone(),
        label: monitor.label(),
        geometry: monitor.geometry(),
        backend,
        value: None,
        applied: None,
        max: None,
        status,
        reason: Some(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(name: &str, serial: &str) -> Monitor {
        Monitor {
            name: name.into(),
            make: "AOC".into(),
            model: "Q27B35E".into(),
            serial: serial.into(),
            focused: false,
            disabled: false,
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
            transform: 0,
        }
    }

    fn display(bus: u32, connector: Option<&str>, serial: &str) -> ddc::Display {
        ddc::Display {
            bus,
            connector: connector.map(Into::into),
            model: "Q27B35E".into(),
            serial: serial.into(),
            problem: None,
        }
    }

    #[test]
    fn connector_wins_and_identical_models_stay_distinct() {
        let displays = [
            display(9, Some("DP-1"), "A"),
            display(11, Some("DP-3"), "B"),
        ];

        assert_eq!(
            find_display(&monitor("DP-3", "B"), &displays).map(|d| d.bus),
            Some(11)
        );
        assert_eq!(
            find_display(&monitor("DP-1", "A"), &displays).map(|d| d.bus),
            Some(9)
        );
    }

    #[test]
    fn falls_back_on_edid_only_without_connector() {
        let displays = [display(9, None, "A"), display(11, Some("DP-9"), "B")];

        assert_eq!(
            find_display(&monitor("DP-1", "A"), &displays).map(|d| d.bus),
            Some(9)
        );
        assert_eq!(find_display(&monitor("DP-1", "B"), &displays), None);
        assert_eq!(
            find_display(&monitor("DP-1", ""), &[display(9, None, "")]),
            None
        );
    }

    #[test]
    fn recognizes_a_permission_problem() {
        assert_eq!(
            status_of("See https://www.ddcutil.com/i2c_permissions"),
            Status::Permission
        );
        assert_eq!(status_of("DDC communication failed"), Status::Unreachable);
    }
}
