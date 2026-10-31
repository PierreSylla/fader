use crate::hypr::Monitor;
use crate::{Error, ddc, hypr};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Backend {
    Ddc { bus: u32 },
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    Ok,
    Unreachable,
    Permission,
}

#[derive(Debug, PartialEq)]
pub struct Screen {
    pub id: String,
    pub label: String,
    pub backend: Backend,
    pub value: Option<u8>,
    pub status: Status,
    pub reason: Option<String>,
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Backend::Ddc { .. } => "ddc",
            Backend::None => "none",
        })
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Status::Ok => "ok",
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
    let detected = ddc::detect();
    // Un `getvcp` prend environ 245 ms (ADR-0003) : les écrans sont lus en parallèle.
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
                    unavailable(
                        monitor,
                        Backend::None,
                        Status::Unreachable,
                        "internal error".to_owned(),
                    )
                })
            })
            .collect()
    }))
}

fn read_screen(monitor: &Monitor, detected: &Result<Vec<ddc::Display>, Error>) -> Screen {
    if monitor.disabled {
        return unavailable(
            monitor,
            Backend::None,
            Status::Unreachable,
            "output is disabled".to_owned(),
        );
    }
    let displays = match detected {
        Ok(displays) => displays,
        Err(e) => {
            return unavailable(
                monitor,
                Backend::None,
                status_of(&e.to_string()),
                e.to_string(),
            );
        }
    };
    let Some(display) = find_display(monitor, displays) else {
        let reason = "no brightness backend found".to_owned();
        return unavailable(monitor, Backend::None, Status::Unreachable, reason);
    };
    let backend = Backend::Ddc { bus: display.bus };
    if let Some(problem) = &display.problem {
        return unavailable(monitor, backend, status_of(problem), problem.clone());
    }
    match ddc::get(display.bus) {
        Ok(brightness) => Screen {
            id: monitor.name.clone(),
            label: monitor.label(),
            backend,
            value: Some(brightness.percent()),
            status: Status::Ok,
            reason: None,
        },
        Err(e) => unavailable(monitor, backend, status_of(&e.to_string()), e.to_string()),
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

fn status_of(message: &str) -> Status {
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
        backend,
        value: None,
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
            disabled: false,
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
