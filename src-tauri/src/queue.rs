//! The download queue scheduler.
//!
//! A download parked with `queue: true` waits here until `start_queue` is
//! called. The scheduler starts queued downloads in order, up to
//! `max_simultaneous` at a time, and starts the next one whenever a
//! queue-managed download finishes. A download started directly by the user
//! (not through the queue) never counts against the limit.
//!
//! The queue holds only data — the order of waiting IDs, the set of IDs it
//! currently has running, the simultaneous limit, and whether it is active.
//! Starting and pausing is orchestrated through `crate::actions`, so the
//! queue never duplicates the engine's resume logic.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use tauri::AppHandle;

use idm_core::downloader::Status;

use crate::actions;
use crate::state::AppState;

/// A snapshot of the queue for the UI.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueSnapshot {
    /// Download IDs waiting to run, highest priority first.
    pub order: Vec<u64>,
    /// Download IDs the queue currently has running.
    pub running: Vec<u64>,
    /// Maximum simultaneous downloads the queue may start.
    pub max_simultaneous: u32,
    /// Whether `start_queue` has been activated.
    pub active: bool,
}

/// The scheduler. All fields are interior-mutable so the Tauri state wrapper
/// can hold one instance for the whole application.
pub struct Queue {
    /// Download IDs waiting to run, highest priority first.
    order: Mutex<Vec<u64>>,
    /// Download IDs the queue currently has running.
    running: Mutex<HashSet<u64>>,
    /// Maximum simultaneous downloads the queue may start.
    max_simultaneous: AtomicU32,
    /// Whether `start_queue` has been activated.
    active: AtomicBool,
}

impl Default for Queue {
    fn default() -> Self {
        Self {
            order: Mutex::new(Vec::new()),
            running: Mutex::new(HashSet::new()),
            max_simultaneous: AtomicU32::new(3),
            active: AtomicBool::new(false),
        }
    }
}

impl Queue {
    /// Park a download in the queue. A download already queued is left alone.
    pub fn enqueue(&self, id: u64) {
        let mut order = self.order.lock().expect("queue order lock poisoned");
        if !order.contains(&id) {
            order.push(id);
        }
    }

    /// Take a download out of the queue. A running queue download is removed
    /// from the running set so a replacement can start.
    pub fn dequeue(&self, id: u64) {
        let mut order = self.order.lock().expect("queue order lock poisoned");
        order.retain(|&x| x != id);
        drop(order);
        let mut running = self.running.lock().expect("queue running lock poisoned");
        running.remove(&id);
    }

    /// Move a queued download to a new position (0 = first). Returns an error
    /// when the download is not queued or the position is out of range.
    pub fn reorder(&self, id: u64, new_position: usize) -> Result<(), String> {
        let mut order = self.order.lock().expect("queue order lock poisoned");
        let idx = order
            .iter()
            .position(|&x| x == id)
            .ok_or_else(|| "this download is not in the queue".to_string())?;
        if new_position >= order.len() {
            return Err("the position is past the end of the queue".to_string());
        }
        order.remove(idx);
        order.insert(new_position, id);
        Ok(())
    }

    /// Set the maximum number of simultaneous downloads the queue may start.
    pub fn set_max_simultaneous(&self, n: u32) {
        self.max_simultaneous.store(n.max(1), Ordering::SeqCst);
    }

    pub fn max_simultaneous(&self) -> u32 {
        self.max_simultaneous.load(Ordering::SeqCst)
    }

    /// Activate or deactivate the queue.
    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::SeqCst);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Replace the waiting order, used when restoring from saved preferences.
    pub(crate) fn set_order(&self, order: Vec<u64>) {
        *self.order.lock().expect("queue order lock poisoned") = order;
    }

    /// Whether a download is currently running because the queue started it.
    pub fn is_running_via_queue(&self, id: u64) -> bool {
        self.running
            .lock()
            .expect("queue running lock poisoned")
            .contains(&id)
    }

    /// A snapshot for the UI.
    pub fn snapshot(&self) -> QueueSnapshot {
        let order = self
            .order
            .lock()
            .expect("queue order lock poisoned")
            .clone();
        let running: Vec<u64> = self
            .running
            .lock()
            .expect("queue running lock poisoned")
            .iter()
            .copied()
            .collect();
        QueueSnapshot {
            order,
            running,
            max_simultaneous: self.max_simultaneous(),
            active: self.is_active(),
        }
    }

    /// Record that a queue-managed download has finished. Returns `true` when
    /// the download was being run by the queue, so the caller knows whether to
    /// try starting the next one.
    pub fn on_finished(&self, id: u64) -> bool {
        let mut running = self.running.lock().expect("queue running lock poisoned");
        if !running.remove(&id) {
            return false;
        }
        true
    }

    /// Activate the queue and start queued downloads up to the limit.
    pub fn start_queue(&self, app: &AppHandle, state: &AppState) {
        self.set_active(true);
        Self::try_start_next(app, state);
    }

    /// Deactivate the queue and pause every download it has running.
    pub fn stop_queue(&self, app: &AppHandle, state: &AppState) {
        self.set_active(false);
        let running: Vec<u64> = self
            .running
            .lock()
            .expect("queue running lock poisoned")
            .iter()
            .copied()
            .collect();
        drop(self.running.lock().expect("queue running lock poisoned"));
        for id in running {
            let _ = actions::pause(app, state, id);
        }
    }

    /// Start the next queued downloads, up to the simultaneous limit.
    ///
    /// A download that is already running (by the queue or directly) does not
    /// take a slot. A download that is not in a resumable state is skipped.
    pub(crate) fn try_start_next(app: &AppHandle, state: &AppState) {
        let queue = state.queue();
        if !queue.is_active() {
            return;
        }

        let running_count = queue
            .running
            .lock()
            .expect("queue running lock poisoned")
            .len() as u32;
        let max = queue.max_simultaneous();
        if running_count >= max {
            return;
        }

        let order = queue
            .order
            .lock()
            .expect("queue order lock poisoned")
            .clone();
        drop(queue.order.lock().expect("queue order lock poisoned"));

        let mut started = 0u32;
        for id in order {
            if running_count + started >= max {
                break;
            }
            if queue.is_running_via_queue(id) {
                continue;
            }
            let download = match state.get(id) {
                Some(d) => d,
                None => continue,
            };
            let status = download.status();
            let resumable = matches!(
                status,
                Status::Queued | Status::Paused | Status::Cancelled | Status::Failed
            );
            if !resumable {
                continue;
            }
            // Claim the slot before spawning, so a concurrent stop cannot
            // start two downloads for one slot.
            queue
                .running
                .lock()
                .expect("queue running lock poisoned")
                .insert(id);
            started += 1;
            let app = app.clone();
            match actions::resume(&app, state, id, true) {
                Ok(()) => {
                    let _ = app;
                }
                Err(_) => {
                    queue
                        .running
                        .lock()
                        .expect("queue running lock poisoned")
                        .remove(&id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_queue() -> Queue {
        Queue::default()
    }

    #[test]
    fn enqueue_keeps_insertion_order_and_skips_duplicates() {
        let q = make_queue();
        q.enqueue(1);
        q.enqueue(2);
        q.enqueue(1);
        assert_eq!(q.snapshot().order, vec![1, 2]);
    }

    #[test]
    fn dequeue_removes_from_order_and_running() {
        let q = make_queue();
        q.enqueue(1);
        q.enqueue(2);
        q.running.lock().unwrap().insert(1);
        q.dequeue(1);
        assert_eq!(q.snapshot().order, vec![2]);
        assert!(!q.is_running_via_queue(1));
    }

    #[test]
    fn reorder_moves_a_download() {
        let q = make_queue();
        q.enqueue(1);
        q.enqueue(2);
        q.enqueue(3);
        q.reorder(3, 0).expect("a move");
        assert_eq!(q.snapshot().order, vec![3, 1, 2]);
    }

    #[test]
    fn reorder_out_of_range_is_an_error() {
        let q = make_queue();
        q.enqueue(1);
        assert!(q.reorder(1, 5).is_err());
    }

    #[test]
    fn reorder_unknown_is_an_error() {
        let q = make_queue();
        assert!(q.reorder(99, 0).is_err());
    }

    #[test]
    fn max_simultaneous_defaults_to_three() {
        let q = make_queue();
        assert_eq!(q.max_simultaneous(), 3);
    }

    #[test]
    fn start_queue_activates() {
        let q = make_queue();
        assert!(!q.is_active());
        q.set_active(true);
        assert!(q.is_active());
    }

    #[test]
    fn on_finished_removes_from_running() {
        let q = make_queue();
        q.running.lock().unwrap().insert(7);
        assert!(q.on_finished(7));
        assert!(!q.on_finished(7)); // not there any more
        assert!(!q.is_running_via_queue(7));
    }

    #[test]
    fn max_simultaneous_can_be_raised_and_lowered() {
        let q = make_queue();
        q.set_max_simultaneous(1);
        assert_eq!(q.max_simultaneous(), 1);
        q.set_max_simultaneous(8);
        assert_eq!(q.max_simultaneous(), 8);
        q.set_max_simultaneous(0);
        assert_eq!(q.max_simultaneous(), 1, "never below one");
    }

    #[test]
    fn start_and_stop_queue_toggle_active() {
        let q = make_queue();
        assert!(!q.is_active());
        q.set_active(true);
        assert!(q.is_active());
        q.set_active(false);
        assert!(!q.is_active());
    }

    #[test]
    fn snapshot_reflects_order_and_running() {
        let q = make_queue();
        q.enqueue(1);
        q.enqueue(2);
        q.running.lock().unwrap().insert(2);
        let snap = q.snapshot();
        assert_eq!(snap.order, vec![1, 2]);
        assert_eq!(snap.running, vec![2]);
        assert_eq!(snap.max_simultaneous, 3);
        assert!(!snap.active);
    }

    #[test]
    fn queue_is_empty_after_enqueue_and_dequeue() {
        let q = make_queue();
        q.enqueue(5);
        assert!(q.snapshot().order == vec![5]);
        q.dequeue(5);
        assert!(q.snapshot().order.is_empty());
    }

    #[test]
    fn dequeue_unknown_is_a_noop() {
        let q = make_queue();
        q.dequeue(99);
        assert!(q.snapshot().order.is_empty());
    }
}
