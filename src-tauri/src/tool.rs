use crate::Error;
use std::io::ErrorKind;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(5);

pub fn run(program: &'static str, args: &[&str]) -> Result<String, Error> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            ErrorKind::NotFound => Error::ToolMissing(program),
            _ => e.into(),
        })?;
    let deadline = Instant::now() + TIMEOUT;
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Err(Error::ToolTimeout(program));
        }
        std::thread::sleep(POLL);
    }
    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(Error::Tool(format!("{stdout}{stderr}").trim().to_owned()))
}
