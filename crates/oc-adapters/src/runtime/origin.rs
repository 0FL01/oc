//! A finite native operation's lifetime, also captured by owned descendants.
//! Esc remains a turn-local cancellation; final TUI/CLI disconnect cancels this
//! complete application-owned lineage without touching independent CLI work.
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Interactive,
    Run,
}

pub(crate) struct Origin {
    pub(crate) id: u64,
    pub(crate) kind: Kind,
    pub(crate) auto_once: AtomicBool,
    pub(crate) approval_consumer: AtomicBool,
    pub(crate) question_consumer: AtomicBool,
    pub(crate) foreground: std::sync::Arc<AtomicBool>,
    pub(crate) flag: AtomicBool,
    cause: std::sync::Mutex<Option<super::stop::Cause>>,
    changed: watch::Sender<bool>,
    active: std::sync::atomic::AtomicUsize,
    settled: tokio::sync::Notify,
    location: Option<std::sync::Arc<crate::application::authority::Location>>,
}

impl Origin {
    pub(crate) fn new(kind: Kind, auto_once: bool) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            kind,
            auto_once: AtomicBool::new(auto_once),
            approval_consumer: AtomicBool::new(false),
            question_consumer: AtomicBool::new(false),
            foreground: std::sync::Arc::new(AtomicBool::new(false)),
            flag: AtomicBool::new(false),
            cause: std::sync::Mutex::new(None),
            changed: watch::channel(false).0,
            active: std::sync::atomic::AtomicUsize::new(0),
            settled: tokio::sync::Notify::new(),
            location: None,
        }
    }

    pub(crate) fn with_foreground(
        kind: Kind,
        auto_once: bool,
        foreground: std::sync::Arc<AtomicBool>,
    ) -> Self {
        let mut origin = Self::new(kind, auto_once);
        origin.foreground = foreground;
        origin
    }

    pub(crate) fn cancel(&self) {
        self.cancel_with_reason(super::stop::Cause::OriginCancelled);
    }

    pub(crate) fn cancel_with_reason(&self, cause: super::stop::Cause) {
        let mut recorded = self.cause.lock().expect("origin stop");
        recorded.get_or_insert(cause);
        self.foreground.store(true, Ordering::Release);
        self.flag.store(true, Ordering::Release);
        self.changed.send_replace(true);
    }

    pub(crate) fn stop_cause(&self) -> super::stop::Cause {
        self.cause
            .lock()
            .expect("origin stop")
            .unwrap_or(super::stop::Cause::OriginCancelled)
    }

    pub(crate) fn at(
        mut self,
        location: std::sync::Arc<crate::application::authority::Location>,
    ) -> Self {
        self.location = Some(location);
        self
    }

    pub(crate) fn location(
        &self,
    ) -> Option<&std::sync::Arc<crate::application::authority::Location>> {
        self.location.as_ref()
    }

    pub(crate) async fn cancelled(&self) {
        let mut changed = self.changed.subscribe();
        while !*changed.borrow_and_update() {
            if changed.changed().await.is_err() {
                return;
            }
        }
    }

    pub(crate) fn work(self: &std::sync::Arc<Self>) -> Work {
        self.active.fetch_add(1, Ordering::AcqRel);
        Work(self.clone())
    }

    pub(crate) async fn settle(&self) {
        loop {
            let done = self.settled.notified();
            tokio::pin!(done);
            done.as_mut().enable();
            if self.active.load(Ordering::Acquire) == 0 {
                return;
            }
            done.await;
        }
    }
}

pub(crate) struct Work(std::sync::Arc<Origin>);
impl Drop for Work {
    fn drop(&mut self) {
        if self.0.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.0.settled.notify_waiters();
        }
    }
}
impl Drop for Origin {
    fn drop(&mut self) {
        if let Some(location) = self.location.take() {
            let retired = location.retired.clone();
            drop(location);
            retired.notify_one();
        }
    }
}

impl std::fmt::Debug for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Origin")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}
