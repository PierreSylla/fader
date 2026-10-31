use crate::i18n::{t, tf};
use crate::screens::{self, Backend, Status};
use crate::{Error, backlight, ddc, hypr, tool};
use std::fmt::Write;
use std::io::ErrorKind;
use std::path::Path;

#[derive(Debug, PartialEq)]
enum Check {
    Ok(String),
    Warn(String),
    Fail { problem: String, fix: String },
}

pub fn run() -> Result<String, Error> {
    let mut checks = vec![check_hyprland(), check_i2c_module(), check_ddcutil()];
    let detected = ddc::detect();
    checks.extend(check_i2c_access(&detected));
    checks.push(check_backlight());
    checks.extend(check_screens());

    let report = format_report(&checks);
    if checks.iter().any(|c| matches!(c, Check::Fail { .. })) {
        std::io::Write::write_all(&mut std::io::stdout(), report.as_bytes())?;
        return Err(Error::Tool(t("problems found").to_owned()));
    }
    Ok(report)
}

fn format_report(checks: &[Check]) -> String {
    let mut report = String::new();
    for check in checks {
        let _ = match check {
            Check::Ok(text) => writeln!(report, "ok    {text}"),
            Check::Warn(text) => writeln!(report, "warn  {text}"),
            Check::Fail { problem, fix } => writeln!(report, "fail  {problem}\n      -> {fix}"),
        };
    }
    report
}

fn fail(problem: String, fix: &'static str) -> Check {
    Check::Fail {
        problem,
        fix: t(fix).to_owned(),
    }
}

fn check_hyprland() -> Check {
    let Ok(version) = hypr::version() else {
        return fail(
            t("cannot reach Hyprland, is it running in this session?").to_owned(),
            "Run fader from a terminal inside your Hyprland session.",
        );
    };
    let config = hypr::config_provider().unwrap_or_else(|e| e.to_string());
    Check::Ok(format!("Hyprland {version} ({config})"))
}

fn check_i2c_module() -> Check {
    if Path::new("/sys/module/i2c_dev").exists() {
        Check::Ok(t("i2c-dev module is loaded").to_owned())
    } else {
        fail(
            t("i2c-dev module is not loaded").to_owned(),
            "Load it with: sudo modprobe i2c-dev",
        )
    }
}

fn check_ddcutil() -> Check {
    match tool::run("ddcutil", &["--version"]) {
        Ok(version) => Check::Ok(version.lines().next().unwrap_or("ddcutil").to_owned()),
        Err(e) => fail(e.to_string(), "Install it with: sudo pacman -S ddcutil"),
    }
}

/// Tests real access: the `uaccess` ACL can grant it without the `i2c` group (ADR-0003).
fn check_i2c_access(detected: &Result<Vec<ddc::Display>, Error>) -> Vec<Check> {
    let Ok(displays) = detected else {
        return Vec::new();
    };
    displays
        .iter()
        .map(|display| {
            let device = format!("/dev/i2c-{}", display.bus);
            let opened = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&device);
            access_check(&device, opened.map(|_| ()).map_err(|e| e.kind()))
        })
        .collect()
}

fn access_check(device: &str, opened: Result<(), ErrorKind>) -> Check {
    match opened {
        Ok(()) => Check::Ok(tf("{} is accessible", device)),
        Err(ErrorKind::PermissionDenied) => fail(
            tf("{}: permission denied", device),
            "Run: sudo usermod -aG i2c $USER, then log out and back in",
        ),
        Err(kind) => Check::Warn(format!("{device}: {kind}")),
    }
}

fn check_backlight() -> Check {
    match backlight::find() {
        Some(device) => Check::Ok(tf("backlight device: {}", &device)),
        None => Check::Ok(t("no backlight device (normal on a desktop)").to_owned()),
    }
}

fn check_screens() -> Vec<Check> {
    let Ok(screens) = screens::read(None) else {
        return Vec::new();
    };
    screens
        .into_iter()
        .map(|s| {
            let name = format!("{} ({})", s.id, s.label);
            let reason = s.reason.unwrap_or_default().replace('\n', " ");
            match (s.status, s.value) {
                (Status::Ok | Status::Busy, Some(value)) => {
                    Check::Ok(format!("{name}: {} {value}%", s.backend))
                }
                (Status::Permission, _) => fail(
                    format!("{name}: {reason}"),
                    "Run: sudo usermod -aG i2c $USER, then log out and back in",
                ),
                _ if s.backend == Backend::None => Check::Warn(format!("{name}: {reason}")),
                _ => fail(
                    format!("{name}: {reason}"),
                    "Enable DDC/CI in the monitor's on-screen menu, and check the cable",
                ),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_permission_problem_with_its_fix() {
        let check = access_check("/dev/i2c-9", Err(ErrorKind::PermissionDenied));

        let report = format_report(&[check]);

        assert!(report.starts_with("fail  /dev/i2c-9"));
        assert!(report.contains("usermod -aG i2c"));
    }

    #[test]
    fn does_not_blame_permissions_for_other_errors() {
        assert!(matches!(access_check("/dev/i2c-9", Ok(())), Check::Ok(_)));
        assert!(matches!(
            access_check("/dev/i2c-9", Err(ErrorKind::NotFound)),
            Check::Warn(_)
        ));
    }
}
