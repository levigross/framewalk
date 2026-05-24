//! Registry for short-lived MCP background tasks.
//!
//! The binary needs deterministic shutdown ownership: any task that
//! clones the transport handle must be aborted or joined before
//! `main.rs` consumes the final `Arc<TransportHandle>` and shuts GDB
//! down.  This registry keeps those best-effort tasks visible to the
//! shutdown sequence instead of fully detaching them.

use std::sync::Mutex;

use tokio::task::JoinHandle;
use tracing::warn;

pub struct BackgroundTasks {
    handles: Mutex<Vec<JoinHandle<()>>>,
}

impl std::fmt::Debug for BackgroundTasks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackgroundTasks")
            .field("pending", &self.pending_count())
            .finish()
    }
}

impl Default for BackgroundTasks {
    fn default() -> Self {
        Self {
            handles: Mutex::new(Vec::new()),
        }
    }
}

impl BackgroundTasks {
    /// Register a task that should not outlive MCP service shutdown.
    pub fn spawn(&self, handle: JoinHandle<()>) {
        let Ok(mut handles) = self.handles.lock() else {
            handle.abort();
            warn!("background task registry mutex poisoned; aborted new task");
            return;
        };
        handles.push(handle);
    }

    /// Abort and await every registered task, draining the registry.
    pub async fn abort_and_wait(&self) {
        let handles = self.take_handles();
        for handle in &handles {
            handle.abort();
        }

        for handle in handles {
            match handle.await {
                Ok(()) => {}
                Err(err) if err.is_cancelled() => {}
                Err(err) => warn!(?err, "background task failed during shutdown"),
            }
        }
    }

    fn take_handles(&self) -> Vec<JoinHandle<()>> {
        let Ok(mut handles) = self.handles.lock() else {
            warn!("background task registry mutex poisoned during shutdown");
            return Vec::new();
        };
        std::mem::take(&mut *handles)
    }

    fn pending_count(&self) -> usize {
        self.handles.lock().map_or(0, |handles| handles.len())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn abort_and_wait_drains_registered_tasks() {
        let tasks = BackgroundTasks::default();
        let completed = Arc::new(AtomicBool::new(false));
        let completed_clone = Arc::clone(&completed);

        tasks.spawn(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            completed_clone.store(true, Ordering::SeqCst);
        }));

        assert_eq!(tasks.pending_count(), 1);
        tasks.abort_and_wait().await;

        assert_eq!(tasks.pending_count(), 0);
        assert!(!completed.load(Ordering::SeqCst));
    }
}
