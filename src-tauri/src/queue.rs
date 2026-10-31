use crate::Error;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

/// One write in flight per screen; while it runs, only the latest value is kept.
pub struct Queue {
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Default)]
struct State {
    pending: Option<u32>,
    writing: bool,
    error: Option<String>,
    written: Option<u32>,
    closed: bool,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait<'a>(&self, state: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        self.changed
            .wait(state)
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl Queue {
    pub fn new(
        mut write: impl FnMut(u32) -> Result<(), Error> + Send + 'static,
        done: impl Fn() + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared::default());
        let worker = Arc::clone(&shared);
        std::thread::spawn(move || {
            loop {
                let value = {
                    let mut state = worker.lock();
                    while state.pending.is_none() && !state.closed {
                        state = worker.wait(state);
                    }
                    let Some(value) = state.pending.take() else {
                        return;
                    };
                    state.writing = true;
                    value
                };
                let result = write(value);
                {
                    let mut state = worker.lock();
                    state.writing = false;
                    match result {
                        Ok(()) => {
                            state.written = Some(value);
                            state.error = None;
                        }
                        Err(e) => state.error = Some(e.to_string()),
                    }
                }
                worker.changed.notify_all();
                done();
            }
        });
        Self { shared }
    }

    pub fn submit(&self, value: u32) {
        self.shared.lock().pending = Some(value);
        self.shared.changed.notify_all();
    }

    pub fn busy(&self) -> bool {
        let state = self.shared.lock();
        state.writing || state.pending.is_some()
    }

    pub fn error(&self) -> Option<String> {
        self.shared.lock().error.clone()
    }

    pub fn written(&self) -> Option<u32> {
        self.shared.lock().written
    }

    pub fn reset_written(&self) {
        self.shared.lock().written = None;
    }

    pub fn flush(&self) {
        let mut state = self.shared.lock();
        while state.writing || state.pending.is_some() {
            state = self.shared.wait(state);
        }
    }
}

#[cfg(test)]
impl Queue {
    pub fn silent(write: impl FnMut(u32) -> Result<(), Error> + Send + 'static) -> Self {
        Self::new(write, || {})
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        self.shared.lock().closed = true;
        self.shared.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn coalesces_values_submitted_during_a_write() {
        let written = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&written);
        let (started, wait_started) = mpsc::channel();
        let (release, wait_release) = mpsc::channel::<()>();
        let queue = Queue::silent(move |value| {
            log.lock().unwrap().push(value);
            started.send(()).unwrap();
            let _ = wait_release.recv_timeout(Duration::from_secs(5));
            Ok(())
        });

        queue.submit(1);
        wait_started.recv().unwrap();
        for value in 2..=20 {
            queue.submit(value);
        }
        assert!(queue.busy());
        release.send(()).unwrap();
        wait_started.recv().unwrap();
        release.send(()).unwrap();
        queue.flush();

        assert_eq!(*written.lock().unwrap(), [1, 20]);
        assert!(!queue.busy());
    }

    #[test]
    fn keeps_the_last_error_until_a_write_succeeds() {
        let queue = Queue::silent(|value| match value {
            0 => Err(Error::Tool("failed".into())),
            _ => Ok(()),
        });

        queue.submit(0);
        queue.flush();
        assert_eq!(queue.error().as_deref(), Some("failed"));

        queue.submit(1);
        queue.flush();
        assert_eq!(queue.error(), None);
        assert_eq!(queue.written(), Some(1));
    }

    #[test]
    fn dropping_the_queue_still_writes_the_pending_value() {
        let (sent, received) = mpsc::channel();
        let queue = Queue::silent(move |value| {
            sent.send(value).unwrap();
            Ok(())
        });

        queue.submit(7);
        drop(queue);

        assert_eq!(received.recv_timeout(Duration::from_secs(5)), Ok(7));
    }
}
