//! Tab read state comes from the authority, never from navigation or a terminal hint.
use super::*;
use oc_core::queries::{SessionActivity, SessionIdleOutcome, WorkspaceSnapshot};
use oc_tui::app::TabUnread;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Activity {
    entries: HashMap<SessionId, SessionActivity>,
    route: Option<SessionId>,
    root: Option<SessionId>,
    focused: bool,
    refresh: bool,
    retry_at: Option<Instant>,
    retry_delay: Duration,
}

impl Activity {
    pub(super) fn apply(&mut self, snapshot: &WorkspaceSnapshot, route: Option<SessionId>) {
        self.entries = snapshot
            .activity
            .iter()
            .map(|activity| (activity.session.clone(), activity.clone()))
            .collect();
        self.route = route;
        self.root = snapshot.selected_root.clone();
        self.refresh = false;
    }

    pub(super) fn unread(&self, session: &SessionId) -> Option<TabUnread> {
        self.entries
            .get(session)
            .and_then(SessionActivity::unread)
            .map(|outcome| match outcome {
                SessionIdleOutcome::Activity => TabUnread::Activity,
                SessionIdleOutcome::Error => TabUnread::Error,
            })
    }

    pub(super) fn busy(&self, session: &SessionId) -> bool {
        self.entries
            .get(session)
            .is_some_and(|activity| activity.busy)
    }

    pub(super) fn focus(&mut self, event: &CEvent) {
        match event {
            CEvent::FocusGained | CEvent::Key(_) => self.focused = true,
            CEvent::FocusLost => self.focused = false,
            _ => {}
        }
    }

    pub(super) fn observe(&mut self, event: &CoreEvent) {
        match event {
            CoreEvent::SessionViewed {
                session,
                viewed_seq,
            } => {
                if let Some(activity) = self.entries.get_mut(session) {
                    activity.viewed_seq = Some(activity.viewed_seq.unwrap_or(0).max(*viewed_seq));
                }
            }
            CoreEvent::TurnStarted { .. }
            | CoreEvent::TurnFinished { .. }
            | CoreEvent::TurnInterrupted { .. }
            | CoreEvent::TurnFailed { .. }
            | CoreEvent::InboxChanged { .. }
            | CoreEvent::ChildNotice(_) => self.refresh = true,
            _ => {}
        }
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.retry_at
    }

    fn retry(&mut self) {
        self.retry_delay = if self.retry_delay.is_zero() {
            Duration::from_millis(250)
        } else {
            (self.retry_delay * 2).min(Duration::from_secs(5))
        };
        self.retry_at = Some(Instant::now() + self.retry_delay);
    }

    pub(super) async fn sync(&mut self, app: &CoreApp, route: Option<SessionId>) -> bool {
        if self.route != route {
            self.route = route.clone();
            self.root = None;
            self.refresh = true;
            self.retry_at = None;
            self.retry_delay = Duration::ZERO;
        }
        if self.retry_at.is_some_and(|at| at > Instant::now()) {
            return false;
        }
        self.retry_at = None;
        let mut changed = false;
        if self.refresh {
            match app.workspace(route.clone()).await {
                Ok(snapshot) => {
                    self.apply(&snapshot, route);
                    self.retry_delay = Duration::ZERO;
                    changed = true;
                }
                Err(_) => {
                    self.retry();
                    return false;
                }
            }
        }
        let viewed = self
            .root
            .as_ref()
            .filter(|_| self.focused)
            .and_then(|root| self.entries.get(root))
            .filter(|activity| activity.unread().is_some())
            .and_then(|activity| {
                activity
                    .idle_seq
                    .map(|idle| (activity.session.clone(), idle))
            });
        if let Some((session, idle)) = viewed {
            match app.session_viewed(session.clone(), idle).await {
                Ok(activity) => {
                    self.entries.insert(session, activity);
                    self.retry_delay = Duration::ZERO;
                    changed = true;
                }
                Err(_) => self.retry(),
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::core_app::InboxMsg;

    #[tokio::test]
    async fn only_confirmed_focus_acknowledges_the_root_and_newer_idle_survives_captured_ack() {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let root = SessionId("root".into());
        let child = SessionId("child".into());
        let activity = SessionActivity {
            session: root.clone(),
            idle_seq: Some(10),
            viewed_seq: None,
            outcome: Some(SessionIdleOutcome::Activity),
            busy: false,
        };
        let mut controls = Activity::default();
        controls.apply(
            &WorkspaceSnapshot {
                revision: 1,
                deck: TabDeckSnapshot::default(),
                turns: Vec::new(),
                inbox: Vec::new(),
                activity: vec![activity.clone()],
                selected_root: Some(root.clone()),
            },
            Some(child.clone()),
        );
        assert!(!controls.sync(&app, Some(child.clone())).await);
        assert!(
            inbox.try_recv().is_err(),
            "initial/unknown focus cannot mark selected content read"
        );
        controls.focus(&CEvent::FocusGained);
        let failed = async {
            let Some(InboxMsg::SessionViewed {
                session,
                idle_seq,
                ack,
            }) = inbox.recv().await
            else {
                panic!("captured viewed ACK")
            };
            assert_eq!(session, root);
            assert_eq!(idle_seq, 10);
            ack.send(Err(CoreError::Shutdown)).unwrap();
        };
        tokio::join!(controls.sync(&app, Some(child.clone())), failed);
        assert_eq!(controls.unread(&root), Some(TabUnread::Activity));
        assert!(controls.deadline().is_some());
        assert!(
            !controls.sync(&app, Some(child.clone())).await,
            "retry is delayed, not a per-frame request"
        );
        controls.retry_at = Some(Instant::now());
        let raced = async {
            let Some(InboxMsg::SessionViewed {
                session,
                idle_seq,
                ack,
            }) = inbox.recv().await
            else {
                panic!("retry captured watermark")
            };
            assert_eq!(session, root);
            assert_eq!(idle_seq, 10);
            ack.send(Ok(SessionActivity {
                idle_seq: Some(20),
                viewed_seq: Some(10),
                outcome: Some(SessionIdleOutcome::Error),
                ..activity.clone()
            }))
            .unwrap();
        };
        let (changed, ()) = tokio::join!(controls.sync(&app, Some(child.clone())), raced);
        assert!(changed);
        assert_eq!(controls.unread(&root), Some(TabUnread::Error));
        controls.focus(&CEvent::FocusLost);
        assert!(!controls.sync(&app, Some(child.clone())).await);
        assert!(
            inbox.try_recv().is_err(),
            "active but blurred remains unread"
        );
        controls.focus(&CEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('x'),
            crossterm::event::KeyModifiers::NONE,
        )));
        let viewed = async {
            let Some(InboxMsg::SessionViewed {
                session,
                idle_seq,
                ack,
            }) = inbox.recv().await
            else {
                panic!("new captured watermark")
            };
            assert_eq!(session, root);
            assert_eq!(idle_seq, 20);
            ack.send(Ok(SessionActivity {
                idle_seq: Some(20),
                viewed_seq: Some(20),
                ..activity
            }))
            .unwrap();
        };
        tokio::join!(controls.sync(&app, Some(child.clone())), viewed);
        controls.observe(&CoreEvent::SessionViewed {
            session: root.clone(),
            viewed_seq: 10,
        });
        assert_eq!(controls.unread(&root), None);
        assert!(!controls.sync(&app, Some(child)).await);
        assert!(
            inbox.try_recv().is_err(),
            "successful ACK is not repeated and cannot resubmit work"
        );
    }
}
