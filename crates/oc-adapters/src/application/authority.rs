//! Concrete native owner scopes. The socket protocol never owns a second Db,
//! Source observer, MCP supervisor, or a frontend's composer state.
use super::*;
use std::path::PathBuf;
use tokio::sync::watch;
pub(crate) mod live;

pub(crate) struct Resources {
    pub(crate) tui_present: AtomicBool,
    pub(crate) mcp_unsafe_retry: Arc<AtomicBool>,
    pub(crate) mcp_cleanup_failed: Arc<AtomicBool>,
    pub(crate) authentication: Arc<authentication::Owner>,
    pub(crate) children: Arc<crate::runtime::children::Jobs>,
    pub(crate) terminals: Arc<terminals::Owner>,
    pub(crate) shell: Arc<crate::shell::jobs::Jobs>,
    pub(crate) compression: Arc<Mutex<()>>,
    pub(crate) approvals: Arc<oc_core::approval::ApprovalQueue>,
    pub(crate) questions: Arc<oc_core::question::QuestionQueue>,
    pub(crate) live: Arc<live::Live>,
    pub(crate) retired: Arc<tokio::sync::Notify>,
    advance: watch::Sender<u64>,
}

impl Resources {
    pub(crate) fn new(db: &Db, events: &broadcast::Sender<CoreEvent>) -> Self {
        Self {
            tui_present: AtomicBool::new(false),
            mcp_unsafe_retry: Arc::default(),
            mcp_cleanup_failed: Arc::default(),
            authentication: Arc::new(authentication::new(db)),
            children: crate::runtime::children::Jobs::new(db),
            terminals: Arc::new(Mutex::new(crate::terminals::Terminals::new(db).map(
                |mut owner| {
                    owner.set_events(events);
                    owner
                },
            ))),
            shell: crate::shell::jobs::Jobs::new(db),
            compression: Arc::new(Mutex::new(())),
            approvals: Arc::default(),
            questions: Arc::default(),
            live: Arc::default(),
            retired: Arc::default(),
            advance: watch::channel(0).0,
        }
    }

    async fn pump(
        &self,
        db: &Db,
        events: &broadcast::Sender<CoreEvent>,
        mut stop: oneshot::Receiver<()>,
        owner: std::sync::Weak<Owner>,
    ) -> Result<(), oc_core::queries::ServiceDiagnostic> {
        loop {
            let shell = &self.shell;
            self.children
                .reap()
                .await
                .map_err(|error| storage_diagnostic(db.root(), &error))?;
            self.children
                .deliver(Some(events))
                .map_err(|error| storage_diagnostic(db.root(), &error))?;
            shell
                .deliver(events)
                .map_err(|error| storage_diagnostic(db.root(), &error))?;
            if let Some(owner) = owner.upgrade() {
                owner.reconcile_parent_notices()?;
            }
            self.advance
                .send_modify(|revision| *revision = revision.wrapping_add(1));
            tokio::select! {
                _ = &mut stop => break,
                () = shell.changed() => {},
                () = self.children.changed() => {},
            }
        }
        self.children
            .shutdown()
            .await
            .map_err(|error| storage_diagnostic(db.root(), &error))?;
        self.children
            .deliver(Some(events))
            .map_err(|error| storage_diagnostic(db.root(), &error))?;
        self.shell
            .shutdown()
            .await
            .map_err(|error| storage_diagnostic(db.root(), &error))?;
        self.shell
            .deliver(events)
            .map_err(|error| storage_diagnostic(db.root(), &error))?;
        Ok(())
    }
}

/// The single concrete application owner. Runtime ports still use the existing
/// CoreApp handle; neither socket clients nor Source attachments reopen storage.
type LocationCell = Arc<tokio::sync::OnceCell<Result<Arc<Location>, SpawnIssue>>>;

pub(crate) struct Owner {
    pub(crate) db: Db,
    pub(crate) events: broadcast::Sender<CoreEvent>,
    pub(crate) resources: Arc<Resources>,
    env: BTreeMap<String, String>,
    seed: Mutex<Option<Composition>>,
    locations: tokio::sync::Mutex<BTreeMap<PathBuf, LocationCell>>,
    roots: Mutex<BTreeMap<String, Reservation>>,
    closing: AtomicBool,
    root_tasks: Mutex<tokio::task::JoinSet<Result<(), oc_core::queries::ServiceDiagnostic>>>,
    root_added: tokio::sync::Notify,
    origins: Mutex<Vec<std::sync::Weak<crate::runtime::origin::Origin>>>,
    pump_stop: Mutex<Option<oneshot::Sender<()>>>,
    pump: Mutex<Option<JoinHandle<Result<(), oc_core::queries::ServiceDiagnostic>>>>,
}

impl Owner {
    pub(crate) fn new(db: Db, env: BTreeMap<String, String>) -> Result<Arc<Self>, SpawnIssue> {
        db.recover_shell_jobs()
            .and_then(|_| db.recover_child_jobs())
            .and_then(|_| db.recover_interrupted_tools())
            .map_err(|_| {
                SpawnIssue::new(
                    SpawnFailure::Recovery,
                    &db.root().to_string_lossy(),
                    &["operations"],
                    oc_core::queries::ServiceCode::RecoveryFailed,
                )
            })?;
        let (events, _) = broadcast::channel(256);
        let resources = Arc::new(Resources::new(&db, &events));
        let (pump_stop, stop) = oneshot::channel();
        let pump_db = db.shared_handle();
        let pump_resources = resources.clone();
        let pump_events = events.clone();
        let owner = Arc::new(Self {
            db,
            events,
            resources,
            env,
            seed: Mutex::new(None),
            locations: tokio::sync::Mutex::new(BTreeMap::new()),
            roots: Mutex::new(BTreeMap::new()),
            closing: AtomicBool::new(false),
            root_tasks: Mutex::new(tokio::task::JoinSet::new()),
            root_added: tokio::sync::Notify::new(),
            origins: Mutex::new(Vec::new()),
            pump_stop: Mutex::new(Some(pump_stop)),
            pump: Mutex::new(None),
        });
        let weak = Arc::downgrade(&owner);
        let pump = tokio::spawn(async move {
            pump_resources
                .pump(&pump_db, &pump_events, stop, weak)
                .await
        });
        *owner.pump.lock().expect("resource pump") = Some(pump);
        Ok(owner)
    }

    pub(crate) fn prime(&self, composition: Composition) {
        *self.seed.lock().expect("startup composition") = Some(composition);
    }

    fn take_seed(&self, project: &Path) -> Option<Composition> {
        let mut seed = self.seed.lock().expect("startup composition");
        if seed
            .as_ref()
            .is_some_and(|composition| composition.project == project)
        {
            seed.take()
        } else {
            None
        }
    }

    pub(crate) fn location<'a>(
        self: &'a Arc<Self>,
        project: &'a Path,
        defer_provider: bool,
    ) -> futures_util::future::BoxFuture<'a, Result<Arc<Location>, SpawnIssue>> {
        // Source creation can be requested by an existing scoped move. Erase
        // that recursion boundary, not its Send/lifetime ownership guarantee.
        Box::pin(async move {
            let project = std::fs::canonicalize(project).map_err(|_| {
                runtime_issue(
                    &project.to_string_lossy(),
                    &["location"],
                    &RuntimeError::InvalidArgs("Location unavailable".into()),
                )
            })?;
            let cell = {
                let mut locations = self.locations.lock().await;
                locations.entry(project.clone()).or_default().clone()
            };
            let result = cell
                .get_or_init(|| async {
                    start(
                        self.db.shared_handle(),
                        self.resources.clone(),
                        &project,
                        self.env.clone(),
                        self.events.clone(),
                        defer_provider,
                        Arc::downgrade(self),
                    )
                    .await
                    .map(Arc::new)
                })
                .await
                .clone();
            if result.is_err() {
                let mut locations = self.locations.lock().await;
                if locations
                    .get(&project)
                    .is_some_and(|stored| Arc::ptr_eq(stored, &cell))
                {
                    locations.remove(&project);
                }
            }
            result
        })
    }

    pub(crate) fn root(&self, session: &str) -> Option<Arc<Root>> {
        self.roots
            .lock()
            .expect("primary roots")
            .get(session)
            .and_then(|entry| match entry {
                Reservation::Running(root) => Some(root.clone()),
                Reservation::Mutation(_) => None,
            })
    }

    pub(crate) async fn settle_origin(&self, origin: u64) {
        loop {
            let changed = self.root_added.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let active = self.roots.lock().expect("primary roots").values().any(
                |entry| matches!(entry, Reservation::Running(root) if root.origin.id == origin),
            );
            if !active {
                return;
            }
            changed.await;
        }
    }

    pub(crate) fn cancel_interactive(&self) {
        let origins: Vec<_> = self
            .origins
            .lock()
            .expect("operation origins")
            .iter()
            .filter_map(std::sync::Weak::upgrade)
            .collect();
        for origin in origins {
            if origin.kind == crate::runtime::origin::Kind::Interactive {
                self.cancel_origin(&origin);
            }
        }
    }

    pub(crate) fn reserve_mutation(self: &Arc<Self>, session: &str) -> Result<Mutation, CoreError> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let mut roots = self.roots.lock().expect("primary roots");
        if self.closing.load(Ordering::Acquire) {
            return Err(CoreError::Shutdown);
        }
        if roots.contains_key(session) {
            return Err(CoreError::TurnBusy);
        }
        let token = NEXT.fetch_add(1, Ordering::Relaxed);
        roots.insert(session.into(), Reservation::Mutation(token));
        Ok(Mutation {
            owner: self.clone(),
            session: session.into(),
            token,
        })
    }

    pub(crate) async fn loaded_locations(&self) -> Vec<Arc<Location>> {
        self.locations
            .lock()
            .await
            .values()
            .filter_map(|cell| cell.get().and_then(|result| result.as_ref().ok()).cloned())
            .collect()
    }

    /// Event-driven retirement, never a project-viewer count or inactivity TTL.
    /// A root's Origin retains its Location through children, shells and titles.
    pub(crate) async fn retire_idle(&self) -> Result<(), oc_core::queries::ServiceDiagnostic> {
        let mut locations = self.locations.lock().await;
        let unused: Vec<_> = locations
            .iter()
            .filter_map(|(path, cell)| {
                if Arc::strong_count(cell) != 1 {
                    return None;
                }
                cell.get()
                    .and_then(|result| result.as_ref().ok())
                    .filter(|location| Arc::strong_count(location) == 1)
                    .map(|_| path.clone())
            })
            .collect();
        for path in unused {
            if let Some(cell) = locations.remove(&path)
                && let Some(Ok(location)) = cell.get()
            {
                let _ = location.app.shutdown().await;
                let guard = location.guard.lock().expect("source guard").take();
                if let Some(guard) = guard {
                    guard.join_diagnostic().await?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn stop_interactive_terminals(&self) -> Result<(), CoreError> {
        terminals::shutdown(&self.resources.terminals)
    }

    /// Auth/session-only leases never load a project, provider catalog or MCP.
    pub(crate) async fn standalone(&self, message: InboxMsg) -> Result<(), CoreError> {
        use oc_core::queries::{AccountAuthSource, AuthAction, ProviderAccounts};
        let scope = || {
            crate::auth::AuthScope::admit("openai", crate::auth::OPENAI_BASE_URL)
                .map_err(|_| CoreError::Application("OpenAI authentication unavailable".into()))
        };
        match message {
            InboxMsg::List { ack } => {
                let result = self
                    .db
                    .list_sessions()
                    .map_err(app_error)
                    .map(|ids| ids.into_iter().map(SessionId).collect());
                let _ = ack.send(result);
            }
            InboxMsg::AuthMethods { provider, ack } if provider == "openai" => {
                let _ = ack.send(scope().map(|scope| scope.methods()));
            }
            InboxMsg::Authenticate {
                provider,
                action,
                ack,
            } if provider == "openai" => {
                let result = async {
                    let owner = self
                        .resources
                        .authentication
                        .as_ref()
                        .as_ref()
                        .map_err(Clone::clone)?;
                    match action {
                        AuthAction::Begin { method, label } => {
                            if !scope()?
                                .methods()
                                .contains(&oc_core::queries::AuthMethod::OAuth(method))
                            {
                                return Err(CoreError::Application(
                                    "OAuth method unavailable".into(),
                                ));
                            }
                            owner.begin(method, label).await.map_err(app_error)
                        }
                        AuthAction::Status { attempt } => owner.status(&attempt).map_err(app_error),
                        AuthAction::Cancel { attempt } => {
                            owner.cancel(&attempt).await.map_err(app_error)
                        }
                    }
                }
                .await;
                let _ = ack.send(result);
            }
            InboxMsg::ProviderAccounts {
                provider,
                action,
                ack,
            } if provider == "openai" => {
                let result = (|| {
                    let scope = scope()?;
                    let created = action
                        .map(|action| accounts::mutate(&self.db, &scope, action))
                        .transpose()?
                        .flatten();
                    let rows = accounts::stored(&self.db, &scope)?;
                    let effective = if rows.iter().any(|row| row.active) {
                        AccountAuthSource::Stored
                    } else if self
                        .env
                        .get("OPENAI_API_KEY")
                        .is_some_and(|key| !key.trim().is_empty())
                    {
                        AccountAuthSource::Environment
                    } else {
                        AccountAuthSource::Missing
                    };
                    Ok(ProviderAccounts {
                        provider,
                        name: "OpenAI".into(),
                        accounts: rows,
                        effective,
                        created,
                    })
                })();
                let _ = ack.send(result);
            }
            _ => {
                return Err(CoreError::Application(
                    "operation requires an admitted project attachment".into(),
                ));
            }
        }
        Ok(())
    }

    pub(crate) async fn cancel_auth_attempts(&self, attempts: Vec<String>) {
        if let Ok(owner) = self.resources.authentication.as_ref() {
            for attempt in attempts {
                let _ = owner.cancel(&attempt).await;
            }
        }
    }

    pub(crate) fn retain_auth_attempts(&self, attempts: &mut Vec<String>) {
        attempts.retain(|id| {
            self.resources
                .authentication
                .as_ref()
                .as_ref()
                .is_ok_and(|owner| owner.status(id).is_ok())
        });
    }

    /// Reservation precedes asynchronous source/profile preparation and lasts
    /// until the worker has actually settled, not merely until its receipt.
    pub(crate) fn begin(
        self: &Arc<Self>,
        location: Arc<Location>,
        session: String,
        origin: Arc<crate::runtime::origin::Origin>,
        approval_mode: u8,
        question_consumer: bool,
        reservation: Option<&Mutation>,
    ) -> Result<Arc<Root>, CoreError> {
        self.begin_inner(
            location,
            session,
            origin,
            (approval_mode, question_consumer),
            reservation,
            None,
        )
    }

    fn begin_inner(
        self: &Arc<Self>,
        location: Arc<Location>,
        session: String,
        origin: Arc<crate::runtime::origin::Origin>,
        consumers: (u8, bool),
        reservation: Option<&Mutation>,
        batch: Option<crate::runtime::children::ParentBatch>,
    ) -> Result<Arc<Root>, CoreError> {
        let (approval_mode, question_consumer) = consumers;
        let mut roots = self.roots.lock().expect("primary roots");
        if self.closing.load(Ordering::Acquire) {
            return Err(CoreError::Shutdown);
        }
        let reserved = reservation.is_some_and(|reservation| reservation.session == session
            && std::ptr::eq(reservation.owner.as_ref(), self.as_ref())
            && matches!(roots.get(&session), Some(Reservation::Mutation(token)) if *token == reservation.token));
        if (roots.contains_key(&session) || reservation.is_some()) && !reserved {
            return Err(CoreError::TurnBusy);
        }
        if !reserved && roots.len() >= MAX_QUEUE_ITEMS {
            return Err(CoreError::QueueFull);
        }
        if let Some(batch) = &batch
            && !self
                .resources
                .children
                .batch_current(batch)
                .map_err(app_error)?
        {
            return Err(CoreError::TurnNotActive);
        }
        let (app, inbox) = CoreApp::channel_with_events(MAX_QUEUE_ITEMS, self.events.clone());
        let (prepared, ready) = watch::channel(None);
        let root = Arc::new(Root {
            app,
            location: location.clone(),
            ready,
            cancel: origin.foreground.clone(),
            origin: origin.clone(),
        });
        roots.insert(session.clone(), Reservation::Running(root.clone()));
        let mut origins = self.origins.lock().expect("operation origins");
        origins.retain(|origin| origin.strong_count() > 0);
        if !origins
            .iter()
            .any(|held| held.ptr_eq(&Arc::downgrade(&origin)))
        {
            origins.push(Arc::downgrade(&origin));
        }
        drop(origins);
        let owner = Arc::downgrade(self);
        let running = root.clone();
        let events = self.events.clone();
        let db = self.db.shared_handle();
        let origin_work = origin.work();
        let children = self.resources.children.clone();
        self.root_tasks
            .lock()
            .expect("primary tasks")
            .spawn(async move {
                let _origin_work = origin_work;
                let mut refused = None;
                let result = async {
                    let seed = match location.primary(session.clone()).await {
                        Ok(seed) => seed,
                        Err(error) => {
                            refused = Some(error.clone());
                            prepared.send_replace(Some(Err(error)));
                            return Ok(());
                        }
                    };
                    prepared.send_replace(Some(Ok(())));
                    let (initial, reply) = if let Some(batch) = &batch {
                        let (ack, reply) = oneshot::channel();
                        (Some(ParentNotices { session:SessionId(session.clone()), messages:batch.messages.clone(), ack }), Some(reply))
                    } else { (None,None) };
                    let result = seed.run(
                        &db,
                        &events,
                        &session,
                        inbox,
                        &running.cancel,
                        origin,
                        approval_mode,
                        question_consumer,
                        initial,
                    )
                    .await;
                    if let Some(reply) = reply {
                        match reply.await { Ok(Ok(_)) => {}, Ok(Err(error)) => refused = Some(error), Err(_) => refused = Some(CoreError::Shutdown) }
                    }
                    result
                }
                .await;
                if let Some(batch) = &batch {
                    children.finish_batch(batch);
                    if let Some(error) = refused
                        && !running.origin.flag.load(Ordering::Acquire) && !running.cancel.load(Ordering::Acquire) {
                        let diagnostic = match error {
                            CoreError::Diagnostic(diagnostic) | CoreError::ProviderUnavailable(diagnostic) => diagnostic,
                            _ => runtime_issue(&running.location.project.to_string_lossy(), &["subagent","continuation"], &RuntimeError::InvalidArgs("native continuation refused".into())).diagnostic,
                        };
                        let _ = events.send(CoreEvent::ParentContinuationRefused { session:SessionId(session.clone()), diagnostic });
                    }
                }
                if let Some(owner) = owner.upgrade() {
                    let mut roots = owner.roots.lock().expect("primary roots");
                    if roots
                        .get(&session)
                        .is_some_and(|current| matches!(current, Reservation::Running(current) if Arc::ptr_eq(current, &running)))
                    {
                        roots.remove(&session);
                    }
                    drop(roots);
                    owner.root_added.notify_waiters();
                    owner.resources.children.notify();
                }
                result
            });
        self.root_added.notify_one();
        Ok(root)
    }

    fn reconcile_parent_notices(
        self: &Arc<Self>,
    ) -> Result<(), oc_core::queries::ServiceDiagnostic> {
        if self.closing.load(Ordering::Acquire) {
            return Ok(());
        }
        for batch in self
            .resources
            .children
            .parent_batches()
            .map_err(|error| storage_diagnostic(self.db.root(), &error))?
        {
            let Some(location) = batch.origin.location() else {
                self.resources.children.finish_batch(&batch);
                continue;
            };
            let origin = batch.origin.clone();
            let approval = if origin.approval_consumer.load(Ordering::Acquire) {
                if origin.auto_once.load(Ordering::Acquire) {
                    2
                } else {
                    1
                }
            } else {
                0
            };
            let questions = origin.question_consumer.load(Ordering::Acquire);
            match self.begin_inner(
                location.clone(),
                batch.parent.clone(),
                origin,
                (approval, questions),
                None,
                Some(batch),
            ) {
                Ok(_)
                | Err(
                    CoreError::TurnBusy
                    | CoreError::QueueFull
                    | CoreError::TurnNotActive
                    | CoreError::Shutdown,
                ) => {}
                Err(_) => {
                    return Err(runtime_issue(
                        "native owner",
                        &["subagent", "continuation"],
                        &RuntimeError::Storage,
                    )
                    .diagnostic);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn cancel_origin(&self, origin: &crate::runtime::origin::Origin) {
        let _roots = self.roots.lock().expect("primary roots");
        origin.cancel();
        self.resources.children.suppress_origin(origin.id);
    }

    /// Session cancellation and automatic root admission share one decision
    /// boundary. An idle cancellation must not race through the Source inbox.
    pub(crate) fn cancel_session(
        &self,
        location: &Location,
        session: &str,
        expected: Option<u64>,
    ) -> Result<(), CoreError> {
        let roots = self.roots.lock().expect("primary roots");
        let running = match roots.get(session) {
            Some(Reservation::Running(root)) => {
                if expected != Some(root.origin.id) || root.location.project != location.project {
                    return Err(CoreError::TurnNotActive);
                }
                root.cancel.store(true, Ordering::Release);
                true
            }
            Some(Reservation::Mutation(_)) => false,
            None => false,
        };
        let children = self.resources.children.cancel_session(session);
        let shell = self.resources.shell.cancel_session(session);
        if running || children || shell {
            Ok(())
        } else {
            Err(CoreError::TurnNotActive)
        }
    }

    /// Check the captured operation under the same lock as root replacement.
    /// No saturated worker inbox or provider future can delay this signal.
    pub(crate) fn cancel_root(
        &self,
        location: &Location,
        session: &str,
        origin: u64,
        turn: Option<&oc_core::core_app::WorkerTurnId>,
    ) -> Result<(), CoreError> {
        let roots = self.roots.lock().expect("primary roots");
        let Some(Reservation::Running(root)) = roots.get(session) else {
            return Err(CoreError::TurnNotActive);
        };
        if root.origin.id != origin || root.location.project != location.project {
            return Err(CoreError::TurnNotActive);
        }
        if let Some(turn) = turn
            && !self
                .db
                .active_turn(&turn.0, session, None)
                .map_err(app_error)?
        {
            return Err(CoreError::TurnNotActive);
        }
        root.cancel.store(true, Ordering::Release);
        self.resources.children.cancel_session(session);
        self.resources.shell.cancel_session(session);
        Ok(())
    }

    pub(crate) async fn next_root(&self) -> Result<(), oc_core::queries::ServiceDiagnostic> {
        loop {
            let added = self.root_added.notified();
            let next = futures_util::future::poll_fn(|context| {
                self.root_tasks
                    .lock()
                    .expect("primary tasks")
                    .poll_join_next(context)
            })
            .await;
            if let Some(result) = next {
                return result.map_err(|_| title_cleanup_failure())?;
            }
            added.await;
        }
    }

    pub(crate) async fn shutdown(&self) -> Result<(), oc_core::queries::ServiceDiagnostic> {
        let roots: Vec<_> = {
            let roots = self.roots.lock().expect("primary roots");
            self.closing.store(true, Ordering::Release);
            self.resources.children.suppress_wakes();
            roots
                .values()
                .filter_map(|entry| match entry {
                    Reservation::Running(root) => Some(root.clone()),
                    Reservation::Mutation(_) => None,
                })
                .collect()
        };
        for root in &roots {
            root.origin.cancel();
            root.cancel.store(true, Ordering::Release);
            let _ = root.app.shutdown().await;
        }
        let mut tasks = std::mem::take(&mut *self.root_tasks.lock().expect("primary tasks"));
        let mut result = Ok(());
        while let Some(joined) = tasks.join_next().await {
            result = result.and(
                joined
                    .map_err(|_| title_cleanup_failure())
                    .and_then(|result| result),
            );
        }
        let locations: Vec<_> = self
            .locations
            .lock()
            .await
            .values()
            .filter_map(|cell| cell.get().and_then(|result| result.as_ref().ok()).cloned())
            .collect();
        for location in &locations {
            let _ = location.app.shutdown().await;
        }
        for location in locations {
            let guard = location.guard.lock().expect("source guard").take();
            if let Some(guard) = guard {
                result = result.and(guard.join_diagnostic().await);
            }
        }
        if let Some(stop) = self.pump_stop.lock().expect("resource pump").take() {
            let _ = stop.send(());
        }
        let pump = self.pump.lock().expect("resource pump").take();
        if let Some(pump) = pump {
            result = result.and(
                pump.await
                    .map_err(|_| title_cleanup_failure())
                    .and_then(|result| result),
            );
        }
        result = result.and(
            terminals::shutdown(&self.resources.terminals).map_err(|_| title_cleanup_failure()),
        );
        result = result.and(
            authentication::shutdown(&self.resources.authentication)
                .await
                .map_err(|_| title_cleanup_failure()),
        );
        result = result.and(
            self.db
                .response_channels
                .shutdown()
                .await
                .map_err(|_| title_cleanup_failure()),
        );
        if self.resources.mcp_cleanup_failed.load(Ordering::SeqCst) {
            result = result.and(Err(runtime_issue(
                "native owner",
                &["mcp", "cleanup"],
                &RuntimeError::McpShutdown,
            )
            .diagnostic));
        }
        result
    }
}

enum Reservation {
    Running(Arc<Root>),
    Mutation(u64),
}
pub(crate) struct Mutation {
    owner: Arc<Owner>,
    session: String,
    token: u64,
}
impl Drop for Mutation {
    fn drop(&mut self) {
        let mut roots = self.owner.roots.lock().expect("primary roots");
        if matches!(roots.get(&self.session), Some(Reservation::Mutation(token)) if *token == self.token)
        {
            roots.remove(&self.session);
        }
        drop(roots);
        self.owner.resources.children.notify();
    }
}

pub(crate) struct Root {
    pub(crate) app: CoreApp,
    pub(crate) location: Arc<Location>,
    ready: watch::Receiver<Option<Result<(), CoreError>>>,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) origin: Arc<crate::runtime::origin::Origin>,
}

impl Root {
    pub(crate) async fn ready(&self) -> Result<(), CoreError> {
        let mut ready = self.ready.clone();
        loop {
            if let Some(result) = ready.borrow().clone() {
                return result;
            }
            ready.changed().await.map_err(|_| CoreError::Shutdown)?;
        }
    }
}

pub(super) struct Operation {
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) advance: watch::Receiver<u64>,
    pub(super) initial_notices: Option<ParentNotices>,
}

pub(super) struct ParentNotices {
    pub(super) session: SessionId,
    pub(super) messages: Vec<String>,
    pub(super) ack: oneshot::Sender<Result<oc_core::core_app::WorkerTurnId, CoreError>>,
}

pub(crate) struct Location {
    pub(crate) retired: Arc<tokio::sync::Notify>,
    pub(crate) project: PathBuf,
    pub(crate) deck_gate: tokio::sync::Mutex<()>,
    pub(crate) app: CoreApp,
    guard: Mutex<Option<WorkerGuard>>,
    pub(crate) notices: Vec<StartupNotice>,
    requests: mpsc::Sender<PrimaryRequest>,
}

impl Location {
    pub(crate) async fn primary(&self, session: String) -> Result<Primary, CoreError> {
        let (ack, reply) = oneshot::channel();
        self.requests
            .send(PrimaryRequest { session, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        reply.await.map_err(|_| CoreError::Shutdown)?
    }
}

pub(crate) struct Source {
    pub(super) owner: std::sync::Weak<Owner>,
    pub(super) resources: Arc<Resources>,
    pub(super) requests: mpsc::Receiver<PrimaryRequest>,
    published: watch::Sender<Option<Arc<Snapshot>>>,
    stamp: Option<crate::runtime::SourceStamp>,
    titles: Option<(
        mpsc::Sender<AutomaticTitleResult>,
        Arc<Mutex<AutomaticTitles>>,
    )>,
}

pub(super) struct PrimaryRequest {
    session: String,
    ack: oneshot::Sender<Result<Primary, CoreError>>,
}

pub(in crate::application) struct Snapshot {
    pub(in crate::application) composition: Composition,
    pub(in crate::application) effective: Effective,
    pub(in crate::application) source_id: u64,
}

pub(crate) struct Primary {
    pub(crate) runtime: Runtime<'static>,
    pub(super) composition: Composition,
    effective: Effective,
    source: watch::Receiver<Option<Arc<Snapshot>>>,
    epoch: Arc<AtomicU64>,
    resources: Arc<Resources>,
    title_tx: mpsc::Sender<AutomaticTitleResult>,
    title_work: Arc<Mutex<AutomaticTitles>>,
}

impl Primary {
    #[allow(clippy::too_many_arguments)]
    async fn run(
        mut self,
        db: &Db,
        events: &broadcast::Sender<CoreEvent>,
        session: &str,
        mut inbox: mpsc::Receiver<InboxMsg>,
        cancel: &Arc<AtomicBool>,
        origin: Arc<crate::runtime::origin::Origin>,
        approval_mode: u8,
        question_consumer: bool,
        initial_notices: Option<ParentNotices>,
    ) -> Result<(), oc_core::queries::ServiceDiagnostic> {
        self.runtime.bind_native_origin(origin.clone());
        self.composition
            .approval_consumer_mode
            .store(approval_mode, Ordering::SeqCst);
        self.composition
            .question_consumer
            .store(question_consumer, Ordering::SeqCst);
        if approval_mode > 0 && initial_notices.is_none() {
            self.runtime.register_approval_consumer(approval_mode == 2);
        }
        if question_consumer && initial_notices.is_none() {
            self.runtime.register_question_consumer();
        }
        self.runtime.set_approval_events(events);
        self.runtime.set_compaction_events(events);
        let mut registry = WorkspaceRegistry::bind(
            self.runtime.generation_id(),
            self.runtime.location(),
            &self.composition.generation,
            workspace_agents(&self.composition),
            skill_metas(&self.composition),
        );
        let mut sessions =
            BTreeMap::from([(self.runtime.location().to_owned(), session.to_owned())]);
        let mut home = BTreeMap::new();
        let suggestions = Arc::new(Mutex::new(SuggestionQueue::default()));
        let (_unused, mut title_rx) = mpsc::channel(1);
        let mut provider = provider_catalog::ProviderWork::idle();
        let mut reload = reload::Coordinator::follow(self.source);
        let mut operation = Operation {
            cancel: cancel.clone(),
            advance: self.resources.advance.subscribe(),
            initial_notices,
        };
        let result = {
            let work = worker(
                &self.runtime,
                db,
                &mut self.composition,
                &mut self.effective,
                &mut registry,
                &mut inbox,
                events,
                &mut sessions,
                &mut home,
                &self.epoch,
                &suggestions,
                &self.title_tx,
                &mut title_rx,
                &self.title_work,
                &mut provider,
                &self.resources.terminals,
                &self.resources.authentication,
                &mut reload,
                None,
                Some(&mut operation),
            );
            tokio::pin!(work);
            tokio::select! {
                result = &mut work => result,
                () = origin.cancelled() => {
                    cancel.store(true, Ordering::Release);
                    self.runtime.cancel_all_compactions();
                    self.runtime.cancel_pending_approvals();
                    work.await
                },
                () = crate::provider::wait_cancel(cancel) => {
                    self.runtime.cancel_all_compactions();
                    self.runtime.cancel_pending_approvals();
                    work.await
                },
            }
        };
        reload.shutdown().await?;
        match result? {
            WorkerOutcome::Stop => Ok(()),
            WorkerOutcome::Move(prepared) => {
                let prepared = *prepared;
                let record = &prepared.record;
                let target = prepared.destination.as_ref().ok_or_else(|| {
                    runtime_issue(
                        self.runtime.location(),
                        &["session_move", "destination"],
                        &RuntimeError::Storage,
                    )
                    .diagnostic
                })?;
                let source = origin.location().ok_or_else(title_cleanup_failure)?;
                let (first, second) = if source.project <= target.project {
                    (source, target)
                } else {
                    (target, source)
                };
                let _first = first.deck_gate.lock().await;
                let _second = if first.project != second.project {
                    Some(second.deck_gate.lock().await)
                } else {
                    None
                };
                let catalog = target
                    .app
                    .catalog()
                    .await
                    .map_err(|_| title_cleanup_failure())?;
                let follow = record.session == session && record.source == self.runtime.location();
                prepared.recheck(db).map_err(|error| {
                    runtime_issue(
                        self.runtime.location(),
                        &["session_move"],
                        &RuntimeError::InvalidArgs(error),
                    )
                    .diagnostic
                })?;
                db.apply_session_move(record)
                    .map_err(|error| storage_diagnostic(db.root(), &error))?;
                let location = follow.then(|| {
                    Box::new(LocationSnapshot {
                        location: record.directory.clone(),
                        generation: catalog.chrome.terminal_generation,
                        session: record.session.clone(),
                        catalog,
                        diagnostics: Vec::new(),
                        notices: target.notices.clone(),
                    })
                });
                self.runtime.emit(
                    events,
                    CoreEvent::SessionMoved {
                        operation: record.operation.clone(),
                        session: SessionId(record.session.clone()),
                        directory: record.directory.clone(),
                        location,
                    },
                );
                for location in [&record.source, &record.directory] {
                    self.runtime.emit(
                        events,
                        CoreEvent::WorkspaceChanged {
                            location: location.clone(),
                        },
                    );
                }
                Ok(())
            }
            _ => Err(runtime_issue(
                self.runtime.location(),
                &["primary", "routing"],
                &RuntimeError::Storage,
            )
            .diagnostic),
        }
    }
}

impl Source {
    pub(super) fn bind_titles(
        &mut self,
        tx: mpsc::Sender<AutomaticTitleResult>,
        work: Arc<Mutex<AutomaticTitles>>,
    ) {
        self.titles = Some((tx, work));
    }
    pub(super) fn publish(
        &mut self,
        runtime: &Runtime<'_>,
        composition: &Composition,
        effective: &Effective,
        _epoch: &Arc<AtomicU64>,
    ) -> Result<(), oc_core::queries::ServiceDiagnostic> {
        if self
            .stamp
            .as_ref()
            .is_some_and(|stamp| runtime.source_is_current(stamp))
        {
            return Ok(());
        }
        let snapshot = composition.primary_snapshot().map_err(|_| {
            runtime_issue(
                runtime.location(),
                &["source", "snapshot"],
                &RuntimeError::Storage,
            )
            .diagnostic
        })?;
        self.published.send_replace(Some(Arc::new(Snapshot {
            composition: snapshot,
            effective: effective.clone(),
            source_id: runtime.generation_id(),
        })));
        self.stamp = Some(runtime.source_stamp());
        Ok(())
    }

    pub(super) fn primary(
        &self,
        request: PrimaryRequest,
        db: &Db,
        runtime: &Runtime<'_>,
        composition: &Composition,
        effective: &Effective,
        epoch: &Arc<AtomicU64>,
    ) {
        let result = (|| {
            let selected = selection::for_turn(db, composition, effective, &request.session)?;
            let composition = composition.primary_snapshot().map_err(app_error)?;
            Ok(Primary {
                runtime: runtime.owned_primary_snapshot(&request.session, selected.clone()),
                composition,
                effective: selected,
                source: self.published.subscribe(),
                epoch: epoch.clone(),
                resources: self.resources.clone(),
                title_tx: self.titles.as_ref().expect("source title owner").0.clone(),
                title_work: self.titles.as_ref().expect("source title owner").1.clone(),
            })
        })();
        let _ = request.ack.send(result);
    }
}

pub(crate) async fn start(
    db: Db,
    resources: Arc<Resources>,
    project: &Path,
    env: BTreeMap<String, String>,
    events: broadcast::Sender<CoreEvent>,
    defer_provider: bool,
    owner: std::sync::Weak<Owner>,
) -> Result<Location, SpawnIssue> {
    let mut composition = match owner.upgrade().and_then(|owner| owner.take_seed(project)) {
        Some(composition) => composition,
        None => composition::load_local_with_env(project, env)
            .await
            .map_err(SpawnIssue::configuration)?,
    };
    composition
        .attach_custom_connections(&db)
        .map_err(SpawnIssue::configuration)?;
    composition
        .resolve_credentials(&db)
        .await
        .map_err(SpawnIssue::configuration)?;
    composition.attach_public_catalog(&db).await;
    if !defer_provider {
        composition.refresh_public_catalog(&db, false).await;
        composition
            .refresh_provider()
            .await
            .map_err(SpawnIssue::configuration)?;
    }
    let notices = composition.startup_notices.clone();
    let (app, inbox) = CoreApp::channel_with_events(MAX_QUEUE_ITEMS, events.clone());
    let (ready, ready_rx) = oneshot::channel();
    let (requests, receiver) = mpsc::channel(MAX_QUEUE_ITEMS);
    let (published, _) = watch::channel(None);
    let source = Source {
        owner,
        resources: resources.clone(),
        requests: receiver,
        published,
        stamp: None,
        titles: None,
    };
    let retired = resources.retired.clone();
    let handle = tokio::spawn(async move {
        let mut reload = reload::Coordinator::default();
        let result = Box::pin(start_worker_inner(
            db,
            composition,
            (inbox, events, ready),
            resources.children.clone(),
            &resources.authentication,
            &mut reload,
            Some(source),
        ))
        .await;
        let stopped = reload.shutdown().await;
        result.and(stopped)
    });
    let guard = WorkerGuard::from_diagnostic_task(handle);
    match ready_rx.await {
        Ok(Ok(_notes)) => Ok(Location {
            retired,
            project: project.to_path_buf(),
            deck_gate: tokio::sync::Mutex::new(()),
            app,
            guard: Mutex::new(Some(guard)),
            notices,
            requests,
        }),
        Ok(Err(error)) => {
            let _ = app.shutdown().await;
            let _ = guard.join_diagnostic().await;
            Err(error)
        }
        Err(_) => {
            let error = guard.join_diagnostic().await.err().unwrap_or_else(|| {
                crate::config::diagnostic::failure(
                    "native source startup",
                    &["owner"],
                    oc_core::queries::ServiceStage::Admission,
                    oc_core::queries::ServiceCode::SourceUnavailable,
                    oc_core::queries::ServiceAction::ReviewConfiguration,
                )
            });
            Err(SpawnIssue {
                category: SpawnFailure::Runtime,
                diagnostic: error,
            })
        }
    }
}

#[cfg(test)]
mod tests;
