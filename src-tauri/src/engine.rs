use crate::ddc::Brightness;
use crate::queue::Queue;
use crate::screens::{self, Backend, Screen, Status};
use crate::{Error, hypr};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Screen(String),
    Focused,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Adjust {
    Absolute(u8),
    Relative(i32),
}

impl Adjust {
    pub fn parse(text: &str) -> Option<Self> {
        if text.starts_with(['+', '-']) {
            return text.parse().ok().map(Adjust::Relative);
        }
        text.parse()
            .ok()
            .filter(|v| *v <= 100)
            .map(Adjust::Absolute)
    }

    fn apply(self, current: u8) -> u8 {
        match self {
            Adjust::Absolute(value) => value,
            Adjust::Relative(delta) => i32::from(current).saturating_add(delta).clamp(0, 100) as u8,
        }
    }
}

fn to_raw(percent: u8, max: u32) -> u32 {
    let raw = (u64::from(percent) * u64::from(max) + 50) / 100;
    raw.min(u64::from(max)) as u32
}

type Listener = Arc<dyn Fn() + Send + Sync>;

struct Entry {
    screen: Screen,
    queue: Option<Queue>,
    last_set: Option<Instant>,
    applied: Option<u8>,
}

impl Entry {
    fn new(screen: Screen, listener: &Listener) -> Self {
        let queue = screen.writable().then(|| {
            let backend = screen.backend.clone();
            let listener = Arc::clone(listener);
            Queue::new(move |raw| screens::write(&backend, raw), move || listener())
        });
        Self {
            screen,
            queue,
            last_set: None,
            applied: None,
        }
    }

    fn view(&self) -> Screen {
        let mut screen = self.screen.clone();
        if let Some(queue) = &self.queue {
            let applied = queue
                .written()
                .zip(screen.max)
                .map(|(current, max)| Brightness { current, max }.percent())
                .or(self.applied);
            if queue.busy() {
                screen.status = Status::Busy;
                screen.applied = applied;
            } else if let Some(error) = queue.error() {
                screen.value = applied.or(screen.value);
                screen.status = screens::status_of(&error);
                screen.reason = Some(error);
            }
        }
        screen
    }

    /// A read that began before the latest write would bring back an old value.
    fn value_is_newer_than(&self, read_started: Instant) -> bool {
        self.queue.as_ref().is_some_and(Queue::busy)
            || self.last_set.is_some_and(|set| set > read_started)
    }
}

pub struct Engine {
    entries: Mutex<Vec<Entry>>,
    listener: Listener,
    ready: Mutex<bool>,
    became_ready: Condvar,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(|| {})
    }
}

impl Engine {
    pub fn new(listener: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            entries: Mutex::default(),
            listener: Arc::new(listener),
            ready: Mutex::new(false),
            became_ready: Condvar::new(),
        }
    }

    pub fn is_ready(&self) -> bool {
        *self.ready.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn wait_ready(&self) {
        let mut ready = self.ready.lock().unwrap_or_else(PoisonError::into_inner);
        while !*ready {
            ready = self
                .became_ready
                .wait(ready)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn refresh(&self, only: Option<&str>) -> Result<(), Error> {
        let result = self.reload(only);
        *self.ready.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.became_ready.notify_all();
        result
    }

    fn reload(&self, only: Option<&str>) -> Result<(), Error> {
        let started = Instant::now();
        let fresh = screens::read(only)?;
        let mut entries = self.lock();
        let mut previous: Vec<Entry> = entries.drain(..).collect();
        for screen in fresh {
            let same = previous
                .iter()
                .position(|e| e.screen.id == screen.id && e.screen.backend == screen.backend);
            let Some(mut entry) = same.map(|i| previous.swap_remove(i)) else {
                entries.push(Entry::new(screen, &self.listener));
                continue;
            };
            let keep_value = entry.value_is_newer_than(started);
            let value = entry.screen.value;
            entry.screen = screen;
            if keep_value {
                entry.screen.value = value;
            }
            if entry.queue.is_none() {
                entry = Entry {
                    last_set: entry.last_set,
                    ..Entry::new(entry.screen, &self.listener)
                };
            }
            entries.push(entry);
        }
        Ok(())
    }

    /// Picks up changes made with the monitor's own buttons, without a full detection.
    pub fn reread(&self) {
        let started = Instant::now();
        let backends: Vec<(String, Backend)> = self
            .lock()
            .iter()
            .filter(|e| e.screen.writable())
            .map(|e| (e.screen.id.clone(), e.screen.backend.clone()))
            .collect();
        let values: Vec<_> = std::thread::scope(|scope| {
            let readers: Vec<_> = backends
                .iter()
                .map(|(id, backend)| (id, scope.spawn(move || screens::read_value(backend))))
                .collect();
            readers
                .into_iter()
                .filter_map(|(id, reader)| Some((id, reader.join().ok()?)))
                .collect()
        });
        let mut entries = self.lock();
        for (id, result) in values {
            let Some(entry) = entries.iter_mut().find(|e| e.screen.id == *id) else {
                continue;
            };
            if entry.value_is_newer_than(started) {
                continue;
            }
            match result {
                Ok(brightness) => {
                    entry.screen.value = Some(brightness.percent());
                    entry.screen.max = Some(brightness.max);
                    entry.screen.status = Status::Ok;
                    entry.screen.reason = None;
                }
                Err(e) => {
                    entry.screen.status = screens::status_of(&e.to_string());
                    entry.screen.reason = Some(e.to_string());
                }
            }
        }
    }

    pub fn screens(&self) -> Vec<Screen> {
        self.lock().iter().map(Entry::view).collect()
    }

    pub fn list(&self) -> String {
        self.lock().iter().map(|e| format_line(&e.view())).collect()
    }

    pub fn get(&self, id: &str) -> Result<String, Error> {
        let entries = self.lock();
        let entry = entries
            .iter()
            .find(|e| e.screen.id == id)
            .ok_or_else(|| Error::UnknownScreen(id.to_owned()))?;
        let screen = entry.view();
        match screen.value {
            Some(value) => Ok(format!("{value}\n")),
            None => Err(unavailable(screen)),
        }
    }

    pub fn set(&self, target: &Target, adjust: Adjust) -> Result<String, Error> {
        let focused = match target {
            Target::Focused => Some(hypr::focused()?),
            _ => None,
        };
        let wanted = |id: &str| match target {
            Target::Screen(name) => id == name,
            Target::Focused => focused.as_deref() == Some(id),
            Target::All => true,
        };
        let mut entries = self.lock();
        let mut output = String::new();
        let mut failures = Vec::new();
        let mut matched = false;
        for entry in entries.iter_mut().filter(|e| wanted(&e.screen.id)) {
            matched = true;
            let (Some(current), Some(max), Some(queue)) =
                (entry.view().value, entry.screen.max, &entry.queue)
            else {
                failures.push(unavailable(entry.view()).to_string());
                continue;
            };
            let value = adjust.apply(current);
            if !queue.busy() {
                queue.reset_written();
                entry.applied = Some(current);
            }
            queue.submit(to_raw(value, max));
            entry.screen.value = Some(value);
            entry.last_set = Some(Instant::now());
            output.push_str(&format!("{}\t{value}\n", entry.screen.id));
        }
        drop(entries);
        (self.listener)();
        if !matched {
            let id = match target {
                Target::Screen(name) => name.clone(),
                _ => focused.unwrap_or_default(),
            };
            return Err(Error::UnknownScreen(id));
        }
        if failures.is_empty() {
            Ok(output)
        } else {
            Err(Error::SetFailed(failures.join("\n")))
        }
    }

    /// Direct mode exits right after the command: pending writes must finish first.
    pub fn finish_writes(&self) -> Result<(), Error> {
        let entries = self.lock();
        let mut failures = Vec::new();
        for entry in entries.iter() {
            if let Some(queue) = &entry.queue {
                queue.flush();
                if let Some(error) = queue.error() {
                    failures.push(format!("{}: {error}", entry.screen.id));
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(Error::SetFailed(failures.join("\n")))
        }
    }
}

fn unavailable(screen: Screen) -> Error {
    let reason = screen.reason.unwrap_or_else(|| screen.status.to_string());
    Error::ScreenUnavailable {
        id: screen.id,
        reason,
    }
}

fn format_line(s: &Screen) -> String {
    let value = s.value.map_or("-".to_owned(), |v| format!("{v}%"));
    let reason = s
        .reason
        .as_ref()
        .map_or(String::new(), |r| format!("\t{}", r.replace('\n', " ")));
    format!(
        "{}\t{}\t{}\t{value}\t{}{reason}\n",
        s.id, s.label, s.backend, s.status
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_adjustments() {
        assert_eq!(Adjust::parse("50"), Some(Adjust::Absolute(50)));
        assert_eq!(Adjust::parse("100"), Some(Adjust::Absolute(100)));
        assert_eq!(Adjust::parse("+5"), Some(Adjust::Relative(5)));
        assert_eq!(Adjust::parse("-5"), Some(Adjust::Relative(-5)));
        assert_eq!(Adjust::parse("101"), None);
        assert_eq!(Adjust::parse("-"), None);
        assert_eq!(Adjust::parse("abc"), None);
    }

    #[test]
    fn relative_adjustments_stay_in_range() {
        assert_eq!(Adjust::Relative(5).apply(50), 55);
        assert_eq!(Adjust::Relative(30).apply(90), 100);
        assert_eq!(Adjust::Relative(-30).apply(10), 0);
        assert_eq!(Adjust::Relative(i32::MAX).apply(50), 100);
    }

    #[test]
    fn raw_values_never_exceed_the_hardware_maximum() {
        assert_eq!(to_raw(85, 100), 85);
        assert_eq!(to_raw(100, 255), 255);
        assert_eq!(to_raw(50, 255), 128);
        assert_eq!(to_raw(0, 255), 0);
        assert_eq!(to_raw(100, u32::MAX), u32::MAX);
    }

    fn screen(id: &str, value: Option<u8>) -> Screen {
        Screen {
            id: id.into(),
            label: "AOC Q27B35E".into(),
            geometry: crate::hypr::Geometry {
                x: 0,
                y: 0,
                width: 2560,
                height: 1440,
            },
            backend: if value.is_some() {
                Backend::Ddc { bus: 9 }
            } else {
                Backend::None
            },
            value,
            applied: None,
            max: value.map(|_| 100),
            status: if value.is_some() {
                Status::Ok
            } else {
                Status::Unreachable
            },
            reason: value.is_none().then(|| "output is disabled".to_owned()),
        }
    }

    fn engine(screens: Vec<Screen>) -> Engine {
        let entries = screens
            .into_iter()
            .map(|screen| Entry {
                queue: screen.writable().then(|| Queue::silent(|_| Ok(()))),
                screen,
                last_set: None,
                applied: None,
            })
            .collect();
        let engine = Engine::default();
        *engine.lock() = entries;
        engine
    }

    #[test]
    fn set_updates_the_cache_at_once_and_accumulates() {
        let engine = engine(vec![screen("DP-1", Some(85))]);
        let target = Target::Screen("DP-1".into());

        assert_eq!(
            engine.set(&target, Adjust::Relative(5)).unwrap(),
            "DP-1\t90\n"
        );
        assert_eq!(
            engine.set(&target, Adjust::Relative(5)).unwrap(),
            "DP-1\t95\n"
        );
        assert_eq!(engine.get("DP-1").unwrap(), "95\n");
    }

    #[test]
    fn all_applies_to_working_screens_and_reports_the_others() {
        let engine = engine(vec![screen("DP-1", Some(85)), screen("DP-2", None)]);

        let error = engine.set(&Target::All, Adjust::Absolute(40)).unwrap_err();

        assert!(error.to_string().contains("DP-2"));
        assert_eq!(engine.get("DP-1").unwrap(), "40\n");
    }

    #[test]
    fn unknown_screen_is_an_error() {
        let engine = engine(vec![screen("DP-1", Some(85))]);
        let target = Target::Screen("HDMI-Z".into());

        assert!(matches!(
            engine.set(&target, Adjust::Absolute(40)),
            Err(Error::UnknownScreen(_))
        ));
        assert!(matches!(engine.get("HDMI-Z"), Err(Error::UnknownScreen(_))));
    }

    #[test]
    fn a_write_in_flight_exposes_the_value_still_applied() {
        let (release, wait_release) = std::sync::mpsc::channel::<()>();
        let (started, wait_started) = std::sync::mpsc::channel();
        let engine = engine(vec![screen("DP-1", Some(85))]);
        engine.lock()[0].queue = Some(Queue::silent(move |_| {
            let _ = started.send(());
            let _ = wait_release.recv_timeout(std::time::Duration::from_secs(5));
            Ok(())
        }));
        let target = Target::Screen("DP-1".into());

        engine.set(&target, Adjust::Absolute(60)).unwrap();
        wait_started.recv().unwrap();
        engine.set(&target, Adjust::Absolute(40)).unwrap();
        let during = engine.screens().remove(0);
        assert_eq!(during.status, Status::Busy);
        assert_eq!((during.value, during.applied), (Some(40), Some(85)));

        release.send(()).unwrap();
        wait_started.recv().unwrap();
        let during = engine.screens().remove(0);
        assert_eq!((during.value, during.applied), (Some(40), Some(60)));

        release.send(()).unwrap();
        engine.finish_writes().unwrap();
        let after = engine.screens().remove(0);
        assert_eq!(after.status, Status::Ok);
        assert_eq!((after.value, after.applied), (Some(40), None));
    }

    #[test]
    fn a_failed_write_goes_back_to_the_real_value() {
        let engine = engine(vec![screen("DP-1", Some(85))]);
        engine.lock()[0].queue = Some(Queue::silent(|raw| match raw {
            30 => Err(Error::Tool("DDC failed".into())),
            _ => Ok(()),
        }));
        let target = Target::Screen("DP-1".into());

        engine.set(&target, Adjust::Absolute(30)).unwrap();
        engine.finish_writes().unwrap_err();
        assert_eq!(engine.get("DP-1").unwrap(), "85\n");

        assert_eq!(
            engine.set(&target, Adjust::Relative(5)).unwrap(),
            "DP-1\t90\n"
        );
        engine.finish_writes().unwrap();
        assert_eq!(engine.get("DP-1").unwrap(), "90\n");
        assert_eq!(engine.screens()[0].reason, None);
    }

    #[test]
    fn a_failed_write_shows_in_the_listing() {
        let mut entry = Entry {
            screen: screen("DP-1", Some(85)),
            queue: Some(Queue::silent(|_| Err(Error::Tool("DDC failed".into())))),
            last_set: None,
            applied: None,
        };
        let queue = entry.queue.take().unwrap();
        queue.submit(1);
        queue.flush();
        entry.queue = Some(queue);

        let view = entry.view();

        assert_eq!(view.status, Status::Unreachable);
        assert_eq!(view.reason.as_deref(), Some("DDC failed"));
    }
}
