//! A single Crossterm reader feeds a bounded event queue while frame rendering
//! and terminal writes run on the main thread.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use gibson::input::Event;

const INPUT_QUEUE_CAPACITY: usize = 512;
const POLL_SLICE: Duration = Duration::from_millis(40);

pub struct InputPump {
    receiver: Receiver<Event>,
    stop: Arc<AtomicBool>,
    errors: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}

impl InputPump {
    pub fn start() -> Self {
        let (sender, receiver) = mpsc::sync_channel(INPUT_QUEUE_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let errors = Arc::new(AtomicU64::new(0));
        let worker = spawn_reader(sender, stop.clone(), errors.clone());
        Self {
            receiver,
            stop,
            errors,
            worker: Some(worker),
        }
    }

    pub fn try_recv(&self) -> Result<Event, TryRecvError> {
        self.receiver.try_recv()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event> {
        self.receiver.recv_timeout(timeout).ok()
    }

    pub fn errors(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        while self.receiver.try_recv().is_ok() {}
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for InputPump {
    fn drop(&mut self) {
        self.stop();
    }
}

fn spawn_reader(
    sender: SyncSender<Event>,
    stop: Arc<AtomicBool>,
    errors: Arc<AtomicU64>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("synesthesia-input".into())
        .spawn(move || {
            while !stop.load(Ordering::Acquire) {
                match gibson::poll_event(POLL_SLICE) {
                    Ok(Some(event)) => {
                        if sender.send(event).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        thread::sleep(Duration::from_millis(5));
                    }
                }
            }
        })
        .expect("create input reader thread")
}
