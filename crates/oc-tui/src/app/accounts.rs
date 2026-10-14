//! One ephemeral secret input, separate from composer, editor undo and copying.
use super::*;
use oc_core::queries::{
    AccountAction, AccountAuthSource, AuthAttempt, AuthAttemptState, AuthMethod,
    CustomProviderAuth, CustomProviderInput, CustomProviderReply, KeyInput, NativeProtocol,
    OAuthMethod, ProviderAccounts, ProviderConnection,
};

/// Correlates a mounted auth surface, never its URL/code/token, with owned work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRequest {
    pub revision: u64,
    pub provider: String,
    pub method: OAuthMethod,
}

fn revision() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[derive(Default)]
pub(super) struct AccountsView {
    provider: String,
    snapshot: Option<ProviderAccounts>,
    connections: Vec<ProviderConnection>,
    search: String,
    cursor: usize,
    form: Form,
    label: String,
    key: String,
    pending: bool,
    adding: bool,
    removing: bool,
    confirmation: Option<String>,
    failed: bool,
    revision: u64,
    connecting: bool,
    methods: Vec<AuthMethod>,
    auth: Option<AuthRequest>,
    attempt: Option<AuthAttempt>,
    mouse_down: Option<AccountHit>,
    custom_name: String,
    custom_base: String,
    custom_cancel: Option<ProbeCancellation>,
    custom_receipt: Option<u64>,
    custom_models: usize,
    custom_diagnostic: Option<oc_core::queries::ServiceDiagnostic>,
}

impl AccountsView {
    fn providers(&self) -> Vec<&ProviderConnection> {
        let search = self.search.to_lowercase();
        self.connections
            .iter()
            .filter(|c| {
                c.name.to_lowercase().contains(&search)
                    || c.provider.to_lowercase().contains(&search)
            })
            .collect()
    }

    fn can_add(&self) -> bool {
        !self
            .snapshot
            .as_ref()
            .is_some_and(|s| s.effective == AccountAuthSource::Anonymous)
    }

    fn input(&mut self) -> Option<(&mut String, usize)> {
        match self.form {
            Form::Providers => Some((&mut self.search, 256)),
            Form::Key | Form::CustomKey => Some((&mut self.key, 16 * 1024)),
            Form::CustomName => Some((&mut self.custom_name, 128)),
            Form::CustomBase => Some((&mut self.custom_base, 4096)),
            Form::Rename(_) => Some((&mut self.label, 128)),
            _ => None,
        }
    }
}

struct ProbeCancellation(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Drop for ProbeCancellation {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

// Never make ephemeral secret material available to Debug or serialization.
impl std::fmt::Debug for AccountsView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountsView")
            .field("pending", &self.pending)
            .finish_non_exhaustive()
    }
}

#[derive(Default, PartialEq, Eq)]
enum Form {
    #[default]
    List,
    Providers,
    Key,
    Methods,
    OAuth,
    Rename(String),
    CustomName,
    CustomBase,
    CustomAuth,
    CustomKey,
    CustomProbe,
    CustomApi,
    CustomSaving,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AccountHit {
    Close,
    Backdrop,
    Row(usize),
}

impl TuiState {
    pub(super) fn open_accounts(&mut self, connect: bool) -> KeyOutcome {
        *self.accounts = AccountsView {
            provider: if connect {
                "opencode-go".into()
            } else {
                self.account_provider()
            },
            form: if connect { Form::Providers } else { Form::List },
            revision: revision(),
            connecting: connect,
            pending: true,
            ..Default::default()
        };
        self.panel = TuiPanel::Accounts;
        KeyOutcome {
            intent: Some(if connect {
                PanelIntent::LoadProviderConnections {
                    revision: self.accounts.revision,
                }
            } else {
                PanelIntent::ProviderAccounts {
                    provider: self.accounts.provider.clone(),
                    revision: self.accounts.revision,
                    action: None,
                }
            }),
            consumed_input: true,
            ..Default::default()
        }
    }

    pub(super) fn clear_accounts(&mut self) {
        *self.accounts = AccountsView::default();
    }

    pub(super) fn open_provider_accounts(&mut self, provider: String) -> KeyOutcome {
        self.clear_accounts();
        self.accounts.provider = provider.clone();
        self.accounts.revision = revision();
        self.accounts.pending = true;
        self.panel = TuiPanel::Accounts;
        KeyOutcome {
            intent: Some(PanelIntent::ProviderAccounts {
                provider,
                revision: self.accounts.revision,
                action: None,
            }),
            ..Default::default()
        }
    }

    pub(crate) fn account_title(&self) -> &'static str {
        if self.accounts.form == Form::Providers {
            "Connect an integration"
        } else if self.custom_provider_revision().is_some() {
            "Connect a custom provider"
        } else {
            "Connect / accounts"
        }
    }

    pub fn apply_provider_connections(&mut self, result: Result<Vec<ProviderConnection>, ()>) {
        if self.panel != TuiPanel::Accounts || self.accounts.form != Form::Providers {
            return;
        }
        self.accounts.failed = result.is_err();
        self.accounts.pending = false;
        self.accounts.connections = result.unwrap_or_default();
        self.accounts.cursor = 0;
    }

    /// Owner acknowledgement contains safe metadata only. The submitted key was
    /// moved out of this view before the intent left it, even when ACK fails.
    pub fn apply_provider_accounts(&mut self, result: Result<ProviderAccounts, ()>) -> bool {
        if self.panel != TuiPanel::Accounts {
            return false;
        }
        let adding = self.accounts.adding && self.accounts.pending;
        let removing = self.accounts.removing && self.accounts.pending;
        self.accounts.pending = false;
        self.accounts.adding = false;
        self.accounts.removing = false;
        self.accounts.key.clear();
        match result {
            Ok(mut snapshot) if snapshot.provider == self.accounts.provider => {
                snapshot
                    .accounts
                    .sort_by(|a, b| a.label.cmp(&b.label).then(a.id.cmp(&b.id)));
                if removing && snapshot.provider == "openai" && snapshot.accounts.is_empty() {
                    self.close_panel();
                    self.push_transient_note("Disconnected OpenAI", NoteVariant::Success);
                    return false;
                }
                let anonymous =
                    self.accounts.connecting && snapshot.effective == AccountAuthSource::Anonymous;
                if self.accounts.connecting {
                    self.accounts.form = if snapshot.accounts.is_empty() {
                        Form::Methods
                    } else {
                        Form::List
                    };
                    self.accounts.connecting = false;
                    self.accounts.cursor = 0;
                }
                self.accounts.snapshot = Some(snapshot);
                self.accounts.failed = false;
                if adding || anonymous {
                    self.clear_accounts();
                    self.panel = TuiPanel::Model;
                    self.select.reset();
                    return true;
                }
            }
            _ => {
                self.accounts.failed = true;
                self.accounts.form = Form::List;
            }
        }
        false
    }

    pub fn account_methods_request(&self) -> Option<(String, u64)> {
        (self.panel == TuiPanel::Accounts && self.accounts.form == Form::Methods)
            .then(|| (self.accounts.provider.clone(), self.accounts.revision))
    }

    pub fn account_surface_revision(&self) -> Option<u64> {
        (self.panel == TuiPanel::Accounts).then_some(self.accounts.revision)
    }

    pub fn custom_provider_revision(&self) -> Option<u64> {
        (self.panel == TuiPanel::Accounts
            && matches!(
                self.accounts.form,
                Form::CustomName
                    | Form::CustomBase
                    | Form::CustomAuth
                    | Form::CustomKey
                    | Form::CustomProbe
                    | Form::CustomApi
                    | Form::CustomSaving
            ))
        .then_some(self.accounts.revision)
    }

    pub fn custom_provider_cancel(
        &self,
        revision: u64,
    ) -> Option<std::sync::Arc<std::sync::atomic::AtomicBool>> {
        (self.custom_provider_revision() == Some(revision))
            .then(|| {
                self.accounts
                    .custom_cancel
                    .as_ref()
                    .map(|cancel| cancel.0.clone())
            })
            .flatten()
    }

    fn probe_custom_provider(&mut self, auth: CustomProviderAuth) -> KeyOutcome {
        self.accounts.custom_cancel = Some(ProbeCancellation(std::sync::Arc::new(
            std::sync::atomic::AtomicBool::new(false),
        )));
        self.accounts.form = Form::CustomProbe;
        self.accounts.pending = true;
        self.accounts.failed = false;
        self.accounts.custom_receipt = None;
        KeyOutcome {
            intent: Some(PanelIntent::ProbeCustomProvider {
                revision: self.accounts.revision,
                input: CustomProviderInput {
                    name: self.accounts.custom_name.clone(),
                    base_url: self.accounts.custom_base.clone(),
                    auth,
                },
            }),
            ..Default::default()
        }
    }

    /// Only the exact mounted form can accept a probe or durable save receipt.
    pub fn apply_custom_provider(
        &mut self,
        revision: u64,
        result: Result<CustomProviderReply, ()>,
    ) -> Option<String> {
        if self.custom_provider_revision() != Some(revision) {
            return None;
        }
        let saving = self.accounts.form == Form::CustomSaving;
        self.accounts.pending = false;
        match result {
            Ok(CustomProviderReply::Probed {
                revision,
                model_count,
                suggested,
                diagnostic,
            }) if self.accounts.form == Form::CustomProbe => {
                self.accounts.custom_receipt = Some(revision);
                self.accounts.custom_models = model_count;
                self.accounts.failed = diagnostic.is_some();
                self.accounts.custom_diagnostic = diagnostic;
                self.accounts.cursor = match suggested {
                    Some(NativeProtocol::Messages) => 2,
                    Some(NativeProtocol::Chat) => 1,
                    _ => 0,
                };
                self.accounts.form = Form::CustomApi;
            }
            Ok(CustomProviderReply::Saved(connection)) if saving => {
                self.clear_accounts();
                self.panel = TuiPanel::Model;
                self.select.reset();
                return Some(connection.provider);
            }
            _ if saving => {
                self.close_panel();
                self.push_transient_note(
                    "Connection save not acknowledged; inspect /connect before retrying",
                    NoteVariant::Error,
                );
            }
            _ => {
                self.accounts.key.clear();
                self.accounts.custom_receipt = None;
                self.accounts.failed = true;
                self.accounts.form = Form::CustomAuth;
                self.accounts.cursor = 0;
            }
        }
        None
    }

    pub fn apply_auth_methods(
        &mut self,
        provider: &str,
        revision: u64,
        result: Result<Vec<AuthMethod>, ()>,
    ) {
        if self.account_methods_request().as_ref() != Some(&(provider.to_owned(), revision)) {
            return;
        }
        self.accounts.methods = result.unwrap_or_default();
        self.accounts.failed = self.accounts.methods.is_empty();
        self.accounts.cursor = 0;
        if let [method] = self.accounts.methods.as_slice() {
            match *method {
                AuthMethod::Key => {
                    self.accounts.label = "default".into();
                    self.accounts.form = Form::Key;
                }
                AuthMethod::OAuth(method) => {
                    self.accounts.form = Form::OAuth;
                    self.accounts.auth = Some(AuthRequest {
                        revision,
                        provider: provider.into(),
                        method,
                    });
                }
            }
        }
    }

    pub fn auth_request(&self) -> Option<&AuthRequest> {
        (self.panel == TuiPanel::Accounts && self.accounts.form == Form::OAuth)
            .then_some(self.accounts.auth.as_ref())
            .flatten()
    }

    /// A parked view is unmounted: discard its active URL/code immediately.
    pub fn retire_auth_surface(&mut self) {
        if self.auth_request().is_some() || self.custom_provider_revision().is_some() {
            self.close_panel();
        }
    }

    /// A URL/code is accessible only while this exact active surface is pending.
    pub fn auth_detail(&self, copy: bool) -> Option<&str> {
        self.auth_request()?;
        let attempt = self.accounts.attempt.as_ref()?;
        if attempt.state != AuthAttemptState::Pending {
            return None;
        }
        if copy {
            attempt.user_code.as_deref().or(attempt.url.as_deref())
        } else {
            attempt.url.as_deref()
        }
    }

    pub fn apply_auth_attempt(&mut self, request: &AuthRequest, attempt: AuthAttempt) {
        if self.auth_request() != Some(request) {
            return;
        }
        let failure = match attempt.state {
            AuthAttemptState::Failed(failure) => Some(failure.message()),
            AuthAttemptState::Expired => Some("Authorization expired"),
            _ => None,
        };
        if let Some(message) = failure {
            self.auth_failure(request, message);
            return;
        }
        self.accounts.attempt = Some(attempt);
    }

    pub fn auth_failure(&mut self, request: &AuthRequest, message: &'static str) {
        if self.auth_request() == Some(request) {
            self.close_panel();
            self.push_transient_note(message, NoteVariant::Error);
        }
    }

    pub(super) fn accounts_mouse(&mut self, event: MouseEvent, area: Rect) -> KeyOutcome {
        use crate::dialog::{DialogFrame, DialogSize};
        let rect = DialogFrame::rect(area, DialogSize::Medium, 16);
        let hit = if !rect.contains((event.column, event.row).into()) {
            Some(AccountHit::Backdrop)
        } else if event.row == rect.y + 1 && event.column >= rect.right().saturating_sub(7) {
            Some(AccountHit::Close)
        } else {
            let header = 1
                + usize::from(self.accounts.snapshot.is_some())
                + usize::from(self.accounts.failed);
            let start = self.accounts.cursor.saturating_sub(3);
            let (offset, visible, count) = match self.accounts.form {
                Form::Providers => {
                    let count = self.accounts.providers().len() + 1;
                    (header + 1, count.saturating_sub(start).min(8), count)
                }
                Form::Methods => {
                    let count = self.accounts.methods.len();
                    (header + 1, count, count)
                }
                Form::List => {
                    let count = self
                        .accounts
                        .snapshot
                        .as_ref()
                        .map_or(0, |s| s.accounts.len());
                    let add = usize::from(self.accounts.can_add());
                    (
                        header,
                        count.saturating_sub(start).min(8) + add,
                        count + add,
                    )
                }
                _ => (0, 0, 0),
            };
            usize::from(event.row.saturating_sub(rect.y + 3))
                .checked_sub(offset)
                .filter(|row| *row < visible && !self.accounts.pending)
                .map(|row| match self.accounts.form {
                    Form::Providers => row + start,
                    Form::List if self.accounts.can_add() && row == 0 => 0,
                    Form::List => row + start,
                    _ => row,
                })
                .filter(|row| *row < count)
                .map(AccountHit::Row)
        };
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => self.accounts.mouse_down = hit,
            MouseEventKind::Up(MouseButton::Left) => {
                if self.accounts.mouse_down.take() == hit {
                    match hit {
                        Some(AccountHit::Close | AccountHit::Backdrop) => self.close_panel(),
                        Some(AccountHit::Row(index)) => {
                            self.accounts.cursor = index;
                            self.accounts.confirmation = None;
                            return self.accounts_key(KeyAction::Enter);
                        }
                        None => {}
                    }
                }
            }
            MouseEventKind::ScrollUp => return self.accounts_key(KeyAction::Up),
            MouseEventKind::ScrollDown => return self.accounts_key(KeyAction::Down),
            _ => {}
        }
        KeyOutcome::default()
    }

    /// Only a matching durable account receipt can leave auth for model browsing.
    pub fn auth_connected(
        &mut self,
        request: &AuthRequest,
        id: &str,
        result: Result<ProviderAccounts, ()>,
    ) -> bool {
        if self.auth_request() != Some(request) {
            return false;
        }
        if !result
            .as_ref()
            .is_ok_and(|s| s.provider == request.provider && s.accounts.iter().any(|a| a.id == id))
        {
            self.auth_failure(request, "Authorization acknowledgement unavailable");
            return false;
        }
        self.clear_accounts();
        self.panel = TuiPanel::Model;
        self.select.reset();
        true
    }

    fn choose_account_method(&mut self) -> KeyOutcome {
        if !self.accounts.can_add() {
            return KeyOutcome::default();
        }
        self.accounts.form = Form::Methods;
        self.accounts.methods.clear();
        self.accounts.label.clear();
        self.accounts.cursor = 0;
        self.accounts.revision = revision();
        self.accounts.confirmation = None;
        KeyOutcome {
            intent: self
                .account_methods_request()
                .map(|(provider, revision)| PanelIntent::LoadAuthMethods { provider, revision }),
            ..Default::default()
        }
    }

    pub(super) fn paste_accounts(&mut self, text: &str) -> KeyOutcome {
        if self.accounts.pending {
            return KeyOutcome::default();
        }
        let Some((value, cap)) = self.accounts.input() else {
            return KeyOutcome::default();
        };
        if !text.chars().any(char::is_control) && value.len().saturating_add(text.len()) <= cap {
            value.push_str(text);
        }
        if self.accounts.form == Form::Providers {
            self.accounts.cursor = 0;
        }
        KeyOutcome::default()
    }

    pub(super) fn accounts_key(&mut self, action: KeyAction) -> KeyOutcome {
        if matches!(
            action,
            KeyAction::Cancel | KeyAction::Interrupt | KeyAction::Quit
        ) {
            self.close_panel();
            if action == KeyAction::Quit {
                self.status = TuiStatus::Quit;
            }
            return KeyOutcome::default();
        }
        if self.accounts.pending {
            return KeyOutcome::default();
        }
        let mut request = None;
        let action = match (&self.accounts.form, action) {
            (Form::List | Form::Methods, KeyAction::Char('j')) => KeyAction::Down,
            (Form::List | Form::Methods, KeyAction::Char('k')) => KeyAction::Up,
            (_, action) => action,
        };
        match (&self.accounts.form, &action) {
            (Form::Providers, KeyAction::Up | KeyAction::Down) => {
                self.accounts.cursor = self
                    .accounts
                    .cursor
                    .saturating_add_signed(if action == KeyAction::Up { -1 } else { 1 })
                    .min(self.accounts.providers().len());
            }
            (Form::Providers, KeyAction::Enter) => {
                if let Some(connection) = self.accounts.providers().get(self.accounts.cursor) {
                    self.accounts.provider = connection.provider.clone();
                    self.accounts.form = Form::List;
                    self.accounts.cursor = 0;
                    self.accounts.pending = true;
                    return KeyOutcome {
                        intent: Some(PanelIntent::ProviderAccounts {
                            provider: self.accounts.provider.clone(),
                            revision: self.accounts.revision,
                            action: None,
                        }),
                        ..Default::default()
                    };
                } else {
                    self.clear_accounts();
                    self.accounts.form = Form::CustomName;
                    self.accounts.revision = revision();
                }
            }
            (Form::Methods, KeyAction::Up | KeyAction::Down) => {
                self.accounts.cursor = self
                    .accounts
                    .cursor
                    .saturating_add_signed(if action == KeyAction::Up { -1 } else { 1 })
                    .min(self.accounts.methods.len().saturating_sub(1));
            }
            (Form::Methods, KeyAction::Enter) => {
                match self.accounts.methods.get(self.accounts.cursor).copied() {
                    Some(AuthMethod::Key) => {
                        self.accounts.label = "default".into();
                        self.accounts.form = Form::Key;
                    }
                    Some(AuthMethod::OAuth(method)) => {
                        self.accounts.form = Form::OAuth;
                        self.accounts.revision = revision();
                        let request = AuthRequest {
                            revision: self.accounts.revision,
                            provider: self.accounts.provider.clone(),
                            method,
                        };
                        self.accounts.auth = Some(request.clone());
                        return KeyOutcome {
                            intent: Some(PanelIntent::BeginAuthentication(request)),
                            ..Default::default()
                        };
                    }
                    None => {}
                }
            }
            (Form::OAuth, KeyAction::Char('o')) if self.auth_detail(false).is_some() => {
                return KeyOutcome {
                    intent: Some(PanelIntent::OpenAuthorization),
                    ..Default::default()
                };
            }
            (Form::OAuth, KeyAction::Char('c')) if self.auth_detail(true).is_some() => {
                return KeyOutcome {
                    intent: Some(PanelIntent::CopyAuthorization),
                    ..Default::default()
                };
            }
            (Form::Key, KeyAction::Enter) if !self.accounts.key.trim().is_empty() => {
                request = Some(AccountAction::AddKey {
                    label: std::mem::take(&mut self.accounts.label).trim().into(),
                    key: KeyInput::new(std::mem::take(&mut self.accounts.key)),
                });
                self.accounts.adding = true;
                self.accounts.form = Form::List;
            }
            (Form::Rename(id), KeyAction::Enter) if !self.accounts.label.trim().is_empty() => {
                request = Some(AccountAction::Rename {
                    id: id.clone(),
                    label: std::mem::take(&mut self.accounts.label).trim().into(),
                });
                self.accounts.form = Form::List;
            }
            (Form::List, KeyAction::Char('a')) => {
                return self.choose_account_method();
            }
            (Form::List, KeyAction::Up | KeyAction::Down) => {
                let count = self
                    .accounts
                    .snapshot
                    .as_ref()
                    .map_or(0, |s| s.accounts.len())
                    + usize::from(self.accounts.can_add());
                self.accounts.cursor = self
                    .accounts
                    .cursor
                    .saturating_add_signed(if action == KeyAction::Up { -1 } else { 1 })
                    .min(count.saturating_sub(1));
                self.accounts.confirmation = None;
            }
            (
                Form::List,
                KeyAction::Enter
                | KeyAction::Char('r')
                | KeyAction::Char('d')
                | KeyAction::Rename
                | KeyAction::DeleteOrQuit,
            ) => {
                if self.accounts.can_add() && self.accounts.cursor == 0 {
                    if action == KeyAction::Enter {
                        return self.choose_account_method();
                    }
                    return KeyOutcome::default();
                }
                if let Some(account) = self.accounts.snapshot.as_ref().and_then(|s| {
                    s.accounts
                        .get(self.accounts.cursor - usize::from(self.accounts.can_add()))
                }) {
                    if action == KeyAction::Enter {
                        if account.active {
                            return KeyOutcome::default();
                        }
                        request = Some(AccountAction::Activate {
                            id: account.id.clone(),
                        });
                    } else if matches!(action, KeyAction::Char('r') | KeyAction::Rename) {
                        self.accounts.label = account.label.clone();
                        self.accounts.form = Form::Rename(account.id.clone());
                    } else if self.accounts.confirmation.as_ref() == Some(&account.id) {
                        request = Some(AccountAction::Remove {
                            id: account.id.clone(),
                            confirmed: true,
                        });
                        self.accounts.removing = true;
                        self.accounts.confirmation = None;
                    } else {
                        self.accounts.confirmation = Some(account.id.clone());
                    }
                }
            }
            (Form::CustomName, KeyAction::Enter)
                if !self.accounts.custom_name.trim().is_empty() =>
            {
                self.accounts.form = Form::CustomBase;
            }
            (Form::CustomBase, KeyAction::Enter) if !self.accounts.custom_base.is_empty() => {
                self.accounts.form = Form::CustomAuth;
                self.accounts.cursor = 0;
            }
            (Form::CustomAuth | Form::CustomApi, KeyAction::Up | KeyAction::Down) => {
                let max = if self.accounts.form == Form::CustomAuth {
                    1
                } else {
                    2
                };
                self.accounts.cursor = self
                    .accounts
                    .cursor
                    .saturating_add_signed(if action == KeyAction::Up { -1 } else { 1 })
                    .min(max);
            }
            (Form::CustomAuth, KeyAction::Enter) => {
                if self.accounts.cursor == 0 {
                    self.accounts.form = Form::CustomKey;
                } else {
                    return self.probe_custom_provider(CustomProviderAuth::Anonymous);
                }
            }
            (Form::CustomKey, KeyAction::Enter) if !self.accounts.key.trim().is_empty() => {
                let key = KeyInput::new(std::mem::take(&mut self.accounts.key));
                return self.probe_custom_provider(CustomProviderAuth::Key(key));
            }
            (Form::CustomApi, KeyAction::Enter) if self.accounts.custom_receipt.is_some() => {
                self.accounts.pending = true;
                self.accounts.form = Form::CustomSaving;
                return KeyOutcome {
                    intent: Some(PanelIntent::SaveCustomProvider {
                        revision: self.accounts.revision,
                        receipt: self.accounts.custom_receipt.expect("checked receipt"),
                        protocol: [
                            NativeProtocol::Responses,
                            NativeProtocol::Chat,
                            NativeProtocol::Messages,
                        ][self.accounts.cursor],
                    }),
                    ..Default::default()
                };
            }
            (Form::CustomApi, KeyAction::Char('r')) if self.accounts.custom_receipt.is_some() => {
                self.accounts.pending = true;
                self.accounts.form = Form::CustomProbe;
                return KeyOutcome {
                    intent: Some(PanelIntent::RetryCustomProvider {
                        revision: self.accounts.revision,
                        receipt: self
                            .accounts
                            .custom_receipt
                            .take()
                            .expect("checked receipt"),
                    }),
                    ..Default::default()
                };
            }
            (Form::CustomApi, KeyAction::Char('e')) => {
                if let Some(cancel) = self.accounts.custom_cancel.take() {
                    cancel.0.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                self.accounts.custom_receipt = None;
                self.accounts.revision = revision();
                self.accounts.form = Form::CustomAuth;
                self.accounts.cursor = 0;
            }
            (_, KeyAction::Char(c)) if !c.is_control() => {
                return self.paste_accounts(&c.to_string());
            }
            (_, KeyAction::Backspace) => {
                if let Some((value, _)) = self.accounts.input() {
                    value.pop();
                }
                if self.accounts.form == Form::Providers {
                    self.accounts.cursor = 0;
                }
            }
            _ => {}
        }
        if request.is_some() {
            self.accounts.pending = true;
            self.accounts.failed = false;
        }
        KeyOutcome {
            intent: request.map(|action| PanelIntent::ProviderAccounts {
                provider: self.accounts.provider.clone(),
                revision: self.accounts.revision,
                action: Some(action),
            }),
            ..Default::default()
        }
    }

    /// Rendering gets masked glyphs, never a reference to the key.
    pub(crate) fn account_lines(&self) -> Vec<String> {
        let view = &self.accounts;
        if self.custom_provider_revision().is_some() {
            let mut lines = vec!["Custom provider".into()];
            match view.form {
                Form::CustomName => {
                    lines.push("Display name".into());
                    lines.push(view.custom_name.clone());
                }
                Form::CustomBase => {
                    lines.push("Literal API base URL (prefix is preserved)".into());
                    lines.push(view.custom_base.clone());
                }
                Form::CustomAuth => {
                    if view.failed {
                        lines.push("Connection operation refused; secret input cleared".into());
                    }
                    lines.push("Choose authentication".into());
                    for (index, label) in ["API key", "No authentication (explicit)"]
                        .iter()
                        .enumerate()
                    {
                        lines.push(format!(
                            "{} {label}",
                            if view.cursor == index { ">" } else { " " }
                        ));
                    }
                }
                Form::CustomKey => {
                    lines.push("API key (masked)".into());
                    lines.push("•".repeat(view.key.chars().count().min(48)));
                }
                Form::CustomProbe => lines.push("Checking GET /models…".into()),
                Form::CustomApi => {
                    if let Some(diagnostic) = &view.custom_diagnostic {
                        lines.push(format!("Discovery unavailable: {:?}", diagnostic.code));
                    } else {
                        lines.push(format!("Catalog: {} models", view.custom_models));
                    }
                    lines.push("Confirm generation API (catalog is not generation proof)".into());
                    for (index, label) in ["Responses", "Chat Completions", "Messages"]
                        .iter()
                        .enumerate()
                    {
                        lines.push(format!(
                            "{} {label}",
                            if view.cursor == index { ">" } else { " " }
                        ));
                    }
                    lines.push("enter save · r retry GET · e change authentication".into());
                }
                Form::CustomSaving => lines.push("Saving connection…".into()),
                _ => unreachable!("custom surface checked"),
            }
            lines.push("esc cancel".into());
            return lines;
        }
        if view.form == Form::OAuth {
            let mut lines = vec![
                view.auth
                    .as_ref()
                    .map_or("OpenAI authorization", |a| a.method.label())
                    .into(),
            ];
            if let Some(attempt) = &view.attempt {
                if let Some(url) = &attempt.url {
                    lines.push(url.clone());
                }
                if let Some(instructions) = &attempt.instructions {
                    lines.push(instructions.clone());
                }
                lines.push(
                    if attempt.state == AuthAttemptState::Complete {
                        "Refreshing accounts…"
                    } else if attempt.url.is_some() {
                        "Waiting for authorization…"
                    } else {
                        "Starting authorization…"
                    }
                    .into(),
                );
                if self.auth_detail(false).is_some() {
                    lines.push("o open · c copy".into());
                }
            } else {
                lines.push("Starting authorization…".into());
            }
            lines.push("esc cancel".into());
            return lines;
        }
        let mut lines = vec![format!("Provider: {}", view.provider)];
        if let Some(snapshot) = &view.snapshot {
            lines.push(format!("Effective auth: {:?}", snapshot.effective));
        }
        if view.failed {
            lines.push("Account operation refused; secret input cleared".into());
        }
        if view.pending {
            lines.push("Working…".into());
            return lines;
        }
        match &view.form {
            Form::Methods => {
                lines.push("Choose authentication method".into());
                for (index, method) in view.methods.iter().enumerate() {
                    lines.push(format!(
                        "{} {}",
                        if index == view.cursor { ">" } else { " " },
                        method.label()
                    ));
                }
            }
            Form::OAuth => unreachable!("active auth surface rendered separately"),
            Form::Providers => {
                lines.push(format!("Search providers: {}", view.search));
                for (index, connection) in view
                    .providers()
                    .into_iter()
                    .map(Some)
                    .chain(std::iter::once(None))
                    .enumerate()
                    .skip(view.cursor.saturating_sub(3))
                    .take(8)
                {
                    let cursor = if index == view.cursor { ">" } else { " " };
                    if let Some(connection) = connection {
                        let category =
                            if matches!(connection.provider.as_str(), "opencode-go" | "openai") {
                                "Popular"
                            } else {
                                "Services"
                            };
                        let connected = !matches!(
                            connection.source,
                            AccountAuthSource::Missing | AccountAuthSource::UnsupportedOAuth
                        );
                        lines.push(format!(
                            "{cursor} {category} · {}{} ({}) [{:?}]",
                            if connected { "✓ " } else { "" },
                            connection.name,
                            connection.provider,
                            connection.source
                        ));
                    } else {
                        lines.push(format!("{cursor} Services · Custom provider…"));
                    }
                }
            }
            Form::Key => {
                lines.push("API key (masked)".into());
                lines.push("•".repeat(view.key.chars().count().min(48)));
            }
            Form::Rename(_) => {
                lines.push("Rename account".into());
                lines.push(view.label.clone());
            }
            Form::List => {
                if view.can_add() {
                    lines.push(format!(
                        "{} Add account",
                        if view.cursor == 0 { ">" } else { " " }
                    ));
                }
                if let Some(snapshot) = &view.snapshot {
                    for (index, account) in snapshot
                        .accounts
                        .iter()
                        .enumerate()
                        .skip(view.cursor.saturating_sub(3))
                        .take(8)
                    {
                        lines.push(format!(
                            "{} {}{} ({:?})",
                            if index + usize::from(view.can_add()) == view.cursor {
                                ">"
                            } else {
                                " "
                            },
                            account.label,
                            if account.active { " [active]" } else { "" },
                            account.kind
                        ));
                    }
                }
                if view.confirmation.is_some() {
                    lines.push("Press d again to confirm removal".into());
                }
                lines.push("a add · enter activate · r rename · d remove".into());
            }
            Form::CustomName
            | Form::CustomBase
            | Form::CustomAuth
            | Form::CustomKey
            | Form::CustomProbe
            | Form::CustomApi
            | Form::CustomSaving => unreachable!("custom surface rendered above"),
        }
        lines.push("enter continue · esc cancel".into());
        lines
    }
}

#[cfg(test)]
#[path = "accounts_tests.rs"]
mod tests;
