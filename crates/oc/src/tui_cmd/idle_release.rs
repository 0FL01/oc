//! Return the TUI's freed heap pages after a burst, without waking the UI.
//!
//! Input and owner events only stamp an atomic; one background task sleeps
//! until the burst has been quiet for `QUIET` and then trims. The UI loop is
//! never scheduled for it (no paint, no terminal bytes), and an idle TUI with
//! no new events has no timer at all.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const QUIET: Duration = Duration::from_millis(500);

struct Inner {
    quiet: Duration,
    start: Instant,
    last_ms: AtomicU64,
    armed: AtomicBool,
    releases: AtomicU64,
    wake: tokio::sync::Notify,
}

pub(super) struct IdleRelease(Arc<Inner>);

impl IdleRelease {
    pub(super) fn spawn() -> Self {
        Self::spawn_with(QUIET)
    }

    fn spawn_with(quiet: Duration) -> Self {
        let inner = Arc::new(Inner {
            quiet,
            start: Instant::now(),
            last_ms: AtomicU64::new(0),
            armed: AtomicBool::new(false),
            releases: AtomicU64::new(0),
            wake: tokio::sync::Notify::new(),
        });
        let task = Arc::clone(&inner);
        // Deliberately plain tokio::spawn: completion must not wake the UI.
        tokio::spawn(async move {
            loop {
                task.wake.notified().await;
                loop {
                    let last = task.last_ms.load(Ordering::Acquire);
                    let due = task.start + Duration::from_millis(last) + task.quiet;
                    if Instant::now() < due {
                        tokio::time::sleep_until(due.into()).await;
                        continue;
                    }
                    oc_adapters::memory::release_free_memory();
                    task.releases.fetch_add(1, Ordering::Relaxed);
                    task.armed.store(false, Ordering::Release);
                    // An event that raced the trim re-arms instead of being lost.
                    if task.last_ms.load(Ordering::Acquire) == last
                        || task.armed.swap(true, Ordering::AcqRel)
                    {
                        break;
                    }
                }
            }
        });
        Self(inner)
    }

    /// Note activity; cheap enough for every input and owner event.
    pub(super) fn touch(&self) {
        let elapsed = self.0.start.elapsed().as_millis() as u64;
        self.0.last_ms.store(elapsed, Ordering::Release);
        if !self.0.armed.swap(true, Ordering::AcqRel) {
            self.0.wake.notify_one();
        }
    }

    pub(super) fn releases(&self) -> u64 {
        self.0.releases.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn one_release_per_quiet_period_and_none_while_idle() {
        let quiet = Duration::from_millis(200);
        let release = IdleRelease::spawn_with(quiet);
        tokio::time::sleep(quiet * 2).await;
        assert_eq!(release.releases(), 0, "no activity, no trim");
        for _ in 0..10 {
            release.touch();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            release.releases(),
            0,
            "a busy burst is never trimmed mid-way"
        );
        tokio::time::sleep(quiet * 4).await;
        assert_eq!(release.releases(), 1);
        tokio::time::sleep(quiet * 2).await;
        assert_eq!(release.releases(), 1, "idle after the trim stays quiet");
        release.touch();
        tokio::time::sleep(quiet * 4).await;
        assert_eq!(release.releases(), 2);
    }
}
