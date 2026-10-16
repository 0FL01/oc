//! One finite logical-step allowance shared by main, child and auxiliary owners.
use super::*;
use std::hash::{BuildHasher, Hasher};

#[derive(Default)]
pub(super) struct RetryPolicy {
    retries: u32,
    random: std::collections::hash_map::RandomState,
}

impl RetryPolicy {
    pub(super) fn decide(
        &mut self,
        error: &crate::provider::ProviderError,
        now: u64,
    ) -> Option<oc_core::queries::RetryFact> {
        let mut rng = self.random.build_hasher();
        rng.write_u32(self.retries);
        let sample = (rng.finish() >> 11) as f64 / (1u64 << 53) as f64;
        self.decide_sample(error, now, sample)
    }

    fn decide_sample(
        &mut self,
        error: &crate::provider::ProviderError,
        now: u64,
        sample: f64,
    ) -> Option<oc_core::queries::RetryFact> {
        let crate::provider::ProviderError::Request(failure) = error else {
            return None;
        };
        if self.retries >= 10 || !failure.retry_eligible() {
            return None;
        }
        let nominal = match self.retries {
            0 => 2000,
            1 => 4000,
            2 => 8000,
            _ => 10000,
        };
        let minimum = if matches!(
            failure.kind,
            crate::provider::FailureKind::RateLimit
                | crate::provider::FailureKind::ProviderInternal
        ) {
            failure.headers.retry_after_ms.unwrap_or(0).min(900_000)
        } else {
            0
        };
        let delay = ((nominal as f64 * (0.8 + 0.4 * sample)).max(minimum as f64)).ceil() as u64;
        self.retries += 1;
        Some(oc_core::queries::RetryFact {
            attempt: self.retries + 1,
            at: now.saturating_add(delay),
            safe_error: error.to_string(),
        })
    }
}

pub(super) async fn wait(retry: &oc_core::queries::RetryFact, cancel: &AtomicBool) -> bool {
    tokio::select! {
        biased;
        () = crate::provider::wait_cancel(cancel) => false,
        () = tokio::time::sleep(Duration::from_millis(retry.at.saturating_sub(millis()))) => !cancel.load(Ordering::Relaxed),
    }
}

impl Runtime<'_> {
    pub(super) fn publish_span_projection(
        &self,
        session: &str,
        turn: &str,
    ) -> Result<(), RuntimeError> {
        if let Some(events) = self.compaction_events.lock().expect("events").as_ref()
            && let Some(projection) = self.db.turn_presentation(session, turn)?
        {
            let _ = events.send(oc_core::core_app::CoreEvent::TurnPresentation {
                session: oc_core::domain::SessionId(session.into()),
                turn: oc_core::core_app::WorkerTurnId(turn.into()),
                projection,
            });
        }
        Ok(())
    }
    pub(super) fn publish_retry(
        &self,
        session: &str,
        log: &TurnLog,
        retry: &oc_core::queries::RetryFact,
    ) -> Result<(), RuntimeError> {
        self.db.checkpoint_retry(
            session,
            &log.turn_id,
            &log.spans.last().expect("active span").id,
            &log.to_json().to_string(),
            retry,
        )?;
        if let Some(events) = self.compaction_events.lock().expect("events").as_ref() {
            let _ = events.send(oc_core::core_app::CoreEvent::RetryScheduled {
                session: oc_core::domain::SessionId(session.into()),
                turn: oc_core::core_app::WorkerTurnId(log.turn_id.clone()),
                span: log.spans.last().expect("active span").id.clone(),
                retry: retry.clone(),
            });
            if let Some(projection) = self.db.turn_presentation(session, &log.turn_id)? {
                let _ = events.send(oc_core::core_app::CoreEvent::TurnPresentation {
                    session: oc_core::domain::SessionId(session.into()),
                    turn: oc_core::core_app::WorkerTurnId(log.turn_id.clone()),
                    projection,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
