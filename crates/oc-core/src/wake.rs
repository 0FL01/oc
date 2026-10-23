//! Process-wide UI wake for completed background work.
//!
//! The interactive loop polls owned job handles and receipts after a wake,
//! instead of busy-checking them on a short timer. Completions bump a
//! generation and wake every waiter; a stale generation resolves at once, so
//! a completion between sampling and waiting is never lost. Spurious wakes are
//! harmless: the loop only re-polls its own lanes.

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::{Notify, oneshot};

static GENERATION: AtomicU64 = AtomicU64::new(0);
static NOTIFY: Notify = Notify::const_new();

/// Record a completion and wake every waiter.
pub fn notify() {
    GENERATION.fetch_add(1, Ordering::AcqRel);
    NOTIFY.notify_waiters();
}

/// Current completion generation; sample before polling lanes.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

/// Resolve once any completion happened after `seen` was sampled.
pub async fn changed(seen: u64) {
    loop {
        let notified = NOTIFY.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if generation() != seen {
            return;
        }
        notified.await;
    }
}

/// `tokio::spawn` that wakes the UI when the job's future completes.
///
/// The wake precedes the handle's `is_finished` by a few instructions; the
/// caller re-checks once shortly after a wake that finished nothing.
pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(async move {
        let output = future.await;
        notify();
        output
    })
}

struct UiWaker;

/// Waker that bumps the UI generation; poll owned handles with it so their
/// completion wakes the loop after the result is actually ready.
pub fn waker() -> &'static std::task::Waker {
    static WAKER: std::sync::LazyLock<std::task::Waker> =
        std::sync::LazyLock::new(|| std::task::Waker::from(std::sync::Arc::new(UiWaker)));
    &WAKER
}

impl std::task::Wake for UiWaker {
    fn wake(self: std::sync::Arc<Self>) {
        notify();
    }
}

/// Nonblocking receipt read that wakes the UI once the owner replies.
///
/// Same outcomes as `try_recv`; an empty receiver additionally registers the
/// process UI waker, so a later reply or a dropped sender bumps the
/// generation without anyone polling on a timer.
pub fn try_reply<T>(
    receiver: &mut oneshot::Receiver<T>,
) -> Result<T, oneshot::error::TryRecvError> {
    match receiver.try_recv() {
        Err(oneshot::error::TryRecvError::Empty) => {
            let mut context = std::task::Context::from_waker(waker());
            match std::pin::Pin::new(receiver).poll(&mut context) {
                std::task::Poll::Ready(Ok(value)) => Ok(value),
                std::task::Poll::Ready(Err(_)) => Err(oneshot::error::TryRecvError::Closed),
                std::task::Poll::Pending => Err(oneshot::error::TryRecvError::Empty),
            }
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn completion_after_sampling_is_never_lost() {
        let seen = generation();
        notify();
        tokio::time::timeout(Duration::from_secs(1), changed(seen))
            .await
            .expect("stale generation resolves at once");
    }

    #[tokio::test]
    async fn pending_receipt_and_spawned_job_wake_a_waiter() {
        let (tx, mut rx) = oneshot::channel::<u8>();
        assert_eq!(try_reply(&mut rx), Err(oneshot::error::TryRecvError::Empty));
        let seen = generation();
        let waiter = tokio::spawn(changed(seen));
        tx.send(7).unwrap();
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(try_reply(&mut rx), Ok(7));
        assert_eq!(
            try_reply(&mut rx),
            Err(oneshot::error::TryRecvError::Closed)
        );
        let (tx, mut rx) = oneshot::channel::<u8>();
        assert!(try_reply(&mut rx).is_err());
        let seen = generation();
        drop(tx);
        tokio::time::timeout(Duration::from_secs(1), changed(seen))
            .await
            .expect("a dropped owner wakes too");
        assert_eq!(
            try_reply(&mut rx),
            Err(oneshot::error::TryRecvError::Closed)
        );
        let seen = generation();
        let job = spawn(async { 3 });
        tokio::time::timeout(Duration::from_secs(1), changed(seen))
            .await
            .unwrap();
        assert_eq!(job.await.unwrap(), 3);
    }

    #[test]
    fn a_ready_receipt_reads_synchronously() {
        let (tx, mut rx) = oneshot::channel::<u8>();
        tx.send(1).unwrap();
        assert_eq!(try_reply(&mut rx), Ok(1));
    }
}
