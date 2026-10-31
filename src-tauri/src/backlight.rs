use crate::{Error, ddc::Brightness, tool};
use std::path::Path;

const ROOT: &str = "/sys/class/backlight";

pub fn is_internal(output: &str) -> bool {
    output.starts_with("eDP-") || output.starts_with("LVDS-")
}

pub fn find() -> Option<String> {
    find_in(Path::new(ROOT))
}

pub fn get(device: &str) -> Result<Brightness, Error> {
    get_in(Path::new(ROOT), device)
}

pub fn set(device: &str, raw: u32) -> Result<(), Error> {
    tool::run("busctl", &set_args(device, &raw.to_string()))?;
    Ok(())
}

fn set_args<'a>(device: &'a str, raw: &'a str) -> [&'a str; 9] {
    [
        "call",
        "org.freedesktop.login1",
        "/org/freedesktop/login1/session/auto",
        "org.freedesktop.login1.Session",
        "SetBrightness",
        "ssu",
        "backlight",
        device,
        raw,
    ]
}

fn find_in(root: &Path) -> Option<String> {
    let mut devices: Vec<String> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with("ddcci"))
        .collect();
    devices.sort();
    devices.into_iter().next()
}

fn get_in(root: &Path, device: &str) -> Result<Brightness, Error> {
    let read = |file: &str| -> Result<u32, Error> {
        let text = std::fs::read_to_string(root.join(device).join(file))?;
        text.trim()
            .parse()
            .map_err(|_| Error::Tool(text.trim().to_owned()))
    };
    let (current, max) = (read("brightness")?, read("max_brightness")?);
    if max == 0 {
        return Err(Error::Tool(format!("{device}: max_brightness is 0")));
    }
    Ok(Brightness { current, max })
}

#[cfg(test)]
mod tests {
    use super::*;

    // No backlight on the dev machine: this temporary tree mocks sysfs.
    fn mock(name: &str, devices: &[(&str, &str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("fader-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (device, current, max) in devices {
            std::fs::create_dir_all(root.join(device)).unwrap();
            std::fs::write(root.join(device).join("brightness"), current).unwrap();
            std::fs::write(root.join(device).join("max_brightness"), max).unwrap();
        }
        root
    }

    #[test]
    fn reads_and_normalizes_a_device() {
        let root = mock("read", &[("intel_backlight", "937\n", "1875\n")]);

        let brightness = get_in(&root, "intel_backlight").unwrap();

        assert_eq!(brightness.current, 937);
        assert_eq!(brightness.percent(), 50);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ignores_ddcci_devices_and_reports_a_broken_one() {
        let root = mock("find", &[("ddcci5", "1", "2"), ("zero", "0", "0")]);

        assert_eq!(find_in(&root).as_deref(), Some("zero"));
        assert!(get_in(&root, "zero").is_err());
        assert!(get_in(&root, "missing").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_internal_outputs_use_the_backlight() {
        assert!(is_internal("eDP-1"));
        assert!(is_internal("LVDS-1"));
        assert!(!is_internal("DP-1"));
    }

    #[test]
    fn calls_logind_with_the_documented_signature() {
        let args = set_args("intel_backlight", "42");
        assert_eq!(
            args[4..],
            ["SetBrightness", "ssu", "backlight", "intel_backlight", "42"]
        );
    }
}
