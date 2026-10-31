use crate::Error;
use std::io::ErrorKind;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(5);

#[derive(Debug, PartialEq)]
pub struct Display {
    pub bus: u32,
    pub connector: Option<String>,
    pub model: String,
    pub serial: String,
    pub problem: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct Brightness {
    pub current: u16,
    pub max: u16,
}

impl Brightness {
    pub fn percent(&self) -> u8 {
        let percent =
            (u32::from(self.current) * 100 + u32::from(self.max) / 2) / u32::from(self.max);
        percent.min(100) as u8
    }
}

pub fn detect() -> Result<Vec<Display>, Error> {
    Ok(parse_detect(&run(&["detect"])?))
}

pub fn get(bus: u32) -> Result<Brightness, Error> {
    let bus = bus.to_string();
    let output = run(&["--bus", &bus, "--brief", "getvcp", "10"])?;
    parse_brightness(&output).ok_or_else(|| Error::Ddc(output.trim().to_owned()))
}

fn run(args: &[&str]) -> Result<String, Error> {
    let mut child = Command::new("ddcutil")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            ErrorKind::NotFound => Error::NoDdcutil,
            _ => e.into(),
        })?;
    let deadline = Instant::now() + TIMEOUT;
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Err(Error::DdcTimeout);
        }
        std::thread::sleep(POLL);
    }
    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(Error::Ddc(format!("{stdout}{stderr}").trim().to_owned()))
}

fn parse_brightness(output: &str) -> Option<Brightness> {
    let fields: Vec<&str> = output.split_whitespace().collect();
    let ["VCP", "10", "C", current, max] = fields[..] else {
        return None;
    };
    let (current, max) = (current.parse().ok()?, max.parse().ok()?);
    (max > 0).then_some(Brightness { current, max })
}

fn parse_detect(output: &str) -> Vec<Display> {
    let mut displays = Vec::new();
    let mut lines = output.lines().peekable();
    while let Some(header) = lines.next() {
        let usable = header.starts_with("Display ");
        let known = usable || header.ends_with(" display");
        if !known || header.starts_with("Phantom") {
            continue;
        }
        let mut bus = None;
        let mut display = Display {
            bus: 0,
            connector: None,
            model: String::new(),
            serial: String::new(),
            problem: (!usable).then(|| header.to_owned()),
        };
        while let Some(line) = lines.next_if(|l| l.starts_with(' ')) {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("I2C bus:") {
                bus = value
                    .trim()
                    .strip_prefix("/dev/i2c-")
                    .and_then(|b| b.parse().ok());
            } else if let Some(value) = line.strip_prefix("DRM_connector:") {
                display.connector = value.trim().split_once('-').map(|(_, c)| c.to_owned());
            } else if let Some(value) = line.strip_prefix("Model:") {
                display.model = value.trim().to_owned();
            } else if let Some(value) = line.strip_prefix("Serial number:") {
                display.serial = value.trim().to_owned();
            } else if !usable && is_reason(line) {
                display.problem = Some(line.to_owned());
            }
        }
        if let Some(bus) = bus {
            display.bus = bus;
            displays.push(display);
        }
    }
    displays
}

fn is_reason(line: &str) -> bool {
    [
        "DDC communication",
        "This ",
        "I2C device is busy",
        "DRM reports",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_real_detect_output() {
        let output = "Display 1
   I2C bus:  /dev/i2c-9
   DRM_connector:           card1-DP-1
   EDID synopsis:
      Mfg id:               AOC - UNK
      Model:                Q27B35E
      Product code:         9987  (0x2703)
      Serial number:        2S6S3HA016250
      Binary serial number: 16250 (0x00003f7a)
      Manufacture year:     2026,  Week: 10
   VCP version:         2.2

Display 2
   I2C bus:  /dev/i2c-10
   DRM_connector:           card1-DP-2
   EDID synopsis:
      Mfg id:               SAM - Samsung Electric Company
      Model:                LC32G5xT
      Serial number:        HK7W100196
   VCP version:         2.0

";
        let displays = parse_detect(output);

        assert_eq!(
            displays[0],
            Display {
                bus: 9,
                connector: Some("DP-1".into()),
                model: "Q27B35E".into(),
                serial: "2S6S3HA016250".into(),
                problem: None,
            }
        );
        assert_eq!(displays[1].bus, 10);
        assert_eq!(displays[1].serial, "HK7W100196");
        assert_eq!(displays.len(), 2);
    }

    #[test]
    fn keeps_the_reason_of_an_invalid_display_and_skips_phantoms() {
        let output = "Invalid display
   I2C bus:  /dev/i2c-4
   DRM_connector:           card0-HDMI-A-1
   EDID synopsis:
      Model:                X
      Serial number:        S1
   DDC communication failed

Phantom display
   Associated non-phantom display: bus /dev/i2c-5
   I2C bus:  /dev/i2c-6

No displays found.
";
        let displays = parse_detect(output);

        assert_eq!(displays.len(), 1);
        assert_eq!(displays[0].connector.as_deref(), Some("HDMI-A-1"));
        assert_eq!(
            displays[0].problem.as_deref(),
            Some("DDC communication failed")
        );
    }

    #[test]
    fn parses_brightness_and_normalizes_to_percent() {
        let brightness = parse_brightness("VCP 10 C 85 100\n").unwrap();
        assert_eq!(
            brightness,
            Brightness {
                current: 85,
                max: 100
            }
        );
        assert_eq!(brightness.percent(), 85);

        assert_eq!(
            Brightness {
                current: 64,
                max: 255
            }
            .percent(),
            25
        );
        assert_eq!(
            Brightness {
                current: 300,
                max: 100
            }
            .percent(),
            100
        );
    }

    #[test]
    fn rejects_unusable_brightness_output() {
        assert_eq!(parse_brightness("VCP 10 ERR"), None);
        assert_eq!(parse_brightness("VCP 10 C 5 0"), None);
        assert_eq!(parse_brightness(""), None);
    }
}
