//! One ephemeral secret input, separate from composer, editor undo and copying.
use super::*;
use oc_core::queries::{
    AccountAction, AccountAuthSource, AuthAttempt, AuthAttemptState, AuthMethod,
    CustomProviderAuth, CustomProviderInput, CustomProviderReply, KeyInput, NativeProtocol,
    OAuthMethod, ProviderAccounts, ProviderConnection,
};
use std::rc::Rc;

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
    fn can_add(&self) -> bool {
        !self
            .snapshot
            .as_ref()
            .is_some_and(|s| s.effective == AccountAuthSource::Anonymous)
    }

    fn input(&mut self) -> Option<(&mut String, usize)> {
        match self.form {
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
        self.select.reset();
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

    /// Display name of the provider this account surface just connected.
    fn connected_provider_name(&self) -> String {
        self.accounts
            .connections
            .iter()
            .find(|c| c.provider == self.accounts.provider)
            .map_or_else(|| self.accounts.provider.clone(), |c| c.name.clone())
    }

    pub(super) fn clear_accounts(&mut self) {
        *self.accounts = AccountsView::default();
    }

    pub(super) fn open_provider_accounts(&mut self, provider: String) -> KeyOutcome {
        self.clear_accounts();
        self.select.reset();
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

    pub fn apply_provider_connections(&mut self, result: Result<Vec<ProviderConnection>, ()>) {
        if self.panel != TuiPanel::Accounts || self.accounts.form != Form::Providers {
            return;
        }
        self.accounts.failed = result.is_err();
        self.accounts.pending = false;
        self.accounts.connections = result.unwrap_or_default();
        self.select.reset();
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
                // Pinned manageConnections: removing the final credential
                // closes the dialog with a success toast.
                if removing && snapshot.accounts.is_empty() {
                    let message = format!("Disconnected {}", snapshot.name);
                    self.close_panel();
                    self.push_transient_note(&message, NoteVariant::Success);
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
                    self.select.reset();
                }
                self.accounts.snapshot = Some(snapshot);
                self.accounts.failed = false;
                if adding || anonymous {
                    let name = self.connected_provider_name();
                    self.clear_accounts();
                    self.open_scoped_model_dialog(name);
                    return true;
                }
            }
            _ => {
                self.accounts.failed = true;
                // KeyMethod keeps its prompt and shows the error under the title.
                self.accounts.form = if adding {
                    self.accounts.label = "default".into();
                    Form::Key
                } else {
                    Form::List
                };
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
                self.select.reset();
                self.select.cursor = match suggested {
                    Some(NativeProtocol::Messages) => 2,
                    Some(NativeProtocol::Chat) => 1,
                    _ => 0,
                };
                self.accounts.form = Form::CustomApi;
            }
            Ok(CustomProviderReply::Saved(connection)) if saving => {
                self.clear_accounts();
                self.open_scoped_model_dialog(connection.name.clone());
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
                self.select.reset();
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
        self.select.reset();
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
        use crate::dialog::{DialogHit, DialogSize};
        let list = self.account_list_form();
        let hit = if list {
            let options = self.modal_options();
            match self
                .select
                .hit(area, DialogSize::Medium, &options, event.column, event.row)
            {
                DialogHit::Backdrop => Some(AccountHit::Backdrop),
                DialogHit::Close => Some(AccountHit::Close),
                DialogHit::Option(index) if !self.accounts.pending => Some(AccountHit::Row(index)),
                _ => None,
            }
        } else {
            let rect = self.account_surface_rect(area);
            if !rect.contains((event.column, event.row).into()) {
                Some(AccountHit::Backdrop)
            } else if event.row == rect.y + 1
                && event.column >= rect.right().saturating_sub(5)
                && event.column < rect.right().saturating_sub(2)
            {
                Some(AccountHit::Close)
            } else {
                None
            }
        };
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => self.accounts.mouse_down = hit,
            MouseEventKind::Up(MouseButton::Left) => {
                if self.accounts.mouse_down.take() == hit {
                    match hit {
                        Some(AccountHit::Close | AccountHit::Backdrop) => self.close_panel(),
                        Some(AccountHit::Row(index)) => {
                            self.select.cursor = index;
                            self.accounts.confirmation = None;
                            return self.accounts_key(KeyAction::Enter);
                        }
                        None => {}
                    }
                }
            }
            // Pinned DialogSelect onMouseMove focuses the hovered row.
            MouseEventKind::Moved => {
                if let Some(AccountHit::Row(index)) = hit
                    && index != self.select.cursor
                {
                    self.select.cursor = index;
                    self.accounts.confirmation = None;
                }
            }
            MouseEventKind::ScrollUp if list => return self.accounts_key(KeyAction::Up),
            MouseEventKind::ScrollDown if list => return self.accounts_key(KeyAction::Down),
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
        let name = self.connected_provider_name();
        self.clear_accounts();
        self.open_scoped_model_dialog(name);
        true
    }

    fn choose_account_method(&mut self) -> KeyOutcome {
        if !self.accounts.can_add() {
            return KeyOutcome::default();
        }
        self.accounts.form = Form::Methods;
        self.accounts.methods.clear();
        self.accounts.label.clear();
        self.select.reset();
        self.accounts.revision = revision();
        self.accounts.confirmation = None;
        KeyOutcome {
            intent: self
                .account_methods_request()
                .map(|(provider, revision)| PanelIntent::LoadAuthMethods { provider, revision }),
            ..Default::default()
        }
    }

    fn account_list_form(&self) -> bool {
        matches!(
            self.accounts.form,
            Form::Providers | Form::List | Form::Methods | Form::CustomAuth | Form::CustomApi
        )
    }

    /// DialogSelect forms with a live Search field (native custom choices keep
    /// their letter actions instead).
    fn account_searchable(&self) -> bool {
        matches!(
            self.accounts.form,
            Form::Providers | Form::List | Form::Methods
        )
    }

    pub(super) fn paste_accounts(&mut self, text: &str) -> KeyOutcome {
        if self.accounts.pending || text.chars().any(char::is_control) {
            return KeyOutcome::default();
        }
        if self.account_searchable() {
            if self.select.query.len().saturating_add(text.len()) <= 256 {
                self.select.query.push_str(text);
                self.select.changed_query();
                self.accounts.confirmation = None;
            }
            return KeyOutcome::default();
        }
        if let Some((value, cap)) = self.accounts.input()
            && value.len().saturating_add(text.len()) <= cap
        {
            value.push_str(text);
        }
        KeyOutcome::default()
    }

    fn focused_account(&self, value: Option<&str>) -> Option<&oc_core::queries::ProviderAccount> {
        let value = value.filter(|value| !value.is_empty())?;
        self.accounts
            .snapshot
            .as_ref()?
            .accounts
            .iter()
            .find(|account| account.id == value)
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
        if self.account_list_form() {
            let options = self.modal_options();
            let count = options.len();
            match action {
                KeyAction::Up | KeyAction::Down | KeyAction::PageUp | KeyAction::PageDown => {
                    self.select.move_by(
                        match action {
                            KeyAction::Up => -1,
                            KeyAction::PageUp => -10,
                            KeyAction::PageDown => 10,
                            _ => 1,
                        },
                        count,
                    );
                    self.select.follow_selection();
                    self.accounts.confirmation = None;
                    return KeyOutcome::default();
                }
                KeyAction::Home | KeyAction::End => {
                    self.select.cursor = if action == KeyAction::Home {
                        0
                    } else {
                        count.saturating_sub(1)
                    };
                    self.select.follow_selection();
                    self.accounts.confirmation = None;
                    return KeyOutcome::default();
                }
                _ => {}
            }
            let focused = options.get(self.select.cursor).map(|o| o.value.clone());
            match (&self.accounts.form, &action) {
                (Form::Providers, KeyAction::Enter) => match focused {
                    Some(provider) if !provider.is_empty() => {
                        self.accounts.provider = provider;
                        self.accounts.form = Form::List;
                        self.select.reset();
                        self.accounts.pending = true;
                        return KeyOutcome {
                            intent: Some(PanelIntent::ProviderAccounts {
                                provider: self.accounts.provider.clone(),
                                revision: self.accounts.revision,
                                action: None,
                            }),
                            ..Default::default()
                        };
                    }
                    Some(_) => {
                        self.clear_accounts();
                        self.select.reset();
                        self.accounts.form = Form::CustomName;
                        self.accounts.revision = revision();
                    }
                    None => {}
                },
                (Form::Methods, KeyAction::Enter) => {
                    let method = focused
                        .and_then(|value| value.parse::<usize>().ok())
                        .and_then(|index| self.accounts.methods.get(index).copied());
                    match method {
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
                (Form::List, KeyAction::Enter) => {
                    if focused.as_deref() == Some("") {
                        return self.choose_account_method();
                    }
                    if let Some(account) = self.focused_account(focused.as_deref())
                        && !account.active
                    {
                        request = Some(AccountAction::Activate {
                            id: account.id.clone(),
                        });
                    }
                }
                (Form::List, KeyAction::Rename) => {
                    if let Some(account) = self.focused_account(focused.as_deref()) {
                        let (label, id) = (account.label.clone(), account.id.clone());
                        self.accounts.label = label;
                        self.accounts.form = Form::Rename(id);
                    }
                }
                (Form::List, KeyAction::DeleteOrQuit) => {
                    if let Some(account) = self.focused_account(focused.as_deref()) {
                        let id = account.id.clone();
                        if self.accounts.confirmation.as_ref() == Some(&id) {
                            request = Some(AccountAction::Remove {
                                id,
                                confirmed: true,
                            });
                            self.accounts.removing = true;
                            self.accounts.confirmation = None;
                        } else {
                            self.accounts.confirmation = Some(id);
                        }
                    }
                }
                (Form::CustomAuth, KeyAction::Enter) => {
                    if self.select.cursor == 0 {
                        self.accounts.form = Form::CustomKey;
                    } else {
                        return self.probe_custom_provider(CustomProviderAuth::Anonymous);
                    }
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
                            ][self.select.cursor.min(2)],
                        }),
                        ..Default::default()
                    };
                }
                (Form::CustomApi, KeyAction::Char('r'))
                    if self.accounts.custom_receipt.is_some() =>
                {
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
                    self.select.reset();
                }
                (_, KeyAction::Char(c)) if !c.is_control() && self.account_searchable() => {
                    return self.paste_accounts(&c.to_string());
                }
                (_, KeyAction::Backspace) if self.account_searchable() => {
                    self.select.query.pop();
                    self.select.changed_query();
                    self.accounts.confirmation = None;
                }
                _ => {}
            }
        } else {
            match (&self.accounts.form, &action) {
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
                    // The prompt stays mounted (busy) until the owner ACK.
                    request = Some(AccountAction::AddKey {
                        label: std::mem::take(&mut self.accounts.label).trim().into(),
                        key: KeyInput::new(std::mem::take(&mut self.accounts.key)),
                    });
                    self.accounts.adding = true;
                }
                (Form::Rename(id), KeyAction::Enter) if !self.accounts.label.trim().is_empty() => {
                    request = Some(AccountAction::Rename {
                        id: id.clone(),
                        label: std::mem::take(&mut self.accounts.label).trim().into(),
                    });
                    self.accounts.form = Form::List;
                }
                (Form::CustomName, KeyAction::Enter)
                    if !self.accounts.custom_name.trim().is_empty() =>
                {
                    self.accounts.form = Form::CustomBase;
                }
                (Form::CustomBase, KeyAction::Enter) if !self.accounts.custom_base.is_empty() => {
                    self.accounts.form = Form::CustomAuth;
                    self.select.reset();
                }
                (Form::CustomKey, KeyAction::Enter) if !self.accounts.key.trim().is_empty() => {
                    let key = KeyInput::new(std::mem::take(&mut self.accounts.key));
                    return self.probe_custom_provider(CustomProviderAuth::Key(key));
                }
                (_, KeyAction::Char(c)) if !c.is_control() => {
                    return self.paste_accounts(&c.to_string());
                }
                (_, KeyAction::Backspace) => {
                    if let Some((value, _)) = self.accounts.input() {
                        value.pop();
                    }
                }
                _ => {}
            }
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

    /// Admitted display name for dialog titles, falling back to the ID.
    fn account_provider_name(&self) -> String {
        self.accounts
            .snapshot
            .as_ref()
            .filter(|snapshot| snapshot.provider == self.accounts.provider)
            .map(|snapshot| snapshot.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.connected_provider_name())
    }

    /// Pinned DialogIntegration rows; the Search filter keeps category groups
    /// in first-ranked order (remeda `groupBy`), not repeated headings.
    pub(crate) fn account_options(&self) -> Rc<Vec<crate::dialog::SelectOption>> {
        use crate::dialog::SelectOption;
        let plain = |value: String, title: &str, category: &str| SelectOption {
            value,
            title: title.into(),
            category: category.into(),
            ..Default::default()
        };
        let options: Vec<SelectOption> = match &self.accounts.form {
            Form::Providers => {
                // integrationOptions: pinned priority, then name and ID.
                let rank = |id: &str| {
                    [
                        "opencode-go",
                        "opencode",
                        "openai",
                        "github-copilot",
                        "anthropic",
                        "google",
                    ]
                    .iter()
                    .position(|popular| *popular == id)
                    .unwrap_or(99)
                };
                let mut rows: Vec<&ProviderConnection> = self.accounts.connections.iter().collect();
                rows.sort_by(|a, b| {
                    rank(&a.provider)
                        .cmp(&rank(&b.provider))
                        .then_with(|| crate::picker::locale_cmp(&a.name, &b.name))
                        .then_with(|| a.provider.cmp(&b.provider))
                });
                rows.into_iter()
                    .map(|connection| SelectOption {
                        value: connection.provider.clone(),
                        title: connection.name.clone(),
                        category: if rank(&connection.provider) < 99 {
                            "Popular"
                        } else {
                            "Services"
                        }
                        .into(),
                        footer: connection.summary.join(", "),
                        check: !connection.summary.is_empty(),
                        ..Default::default()
                    })
                    // Native custom-connection entry (no pinned equivalent).
                    .chain(std::iter::once(plain(
                        String::new(),
                        "Custom provider…",
                        "Services",
                    )))
                    .collect()
            }
            Form::List => {
                let mut rows = Vec::new();
                if self.accounts.can_add() {
                    rows.push(plain(String::new(), "Add account", ""));
                }
                if let Some(snapshot) = &self.accounts.snapshot {
                    rows.extend(snapshot.accounts.iter().map(|account| {
                        let confirming = self.accounts.confirmation.as_ref() == Some(&account.id);
                        SelectOption {
                            value: account.id.clone(),
                            title: if confirming {
                                "Press ctrl+d again to confirm".into()
                            } else {
                                account.label.clone()
                            },
                            category: "Connected accounts".into(),
                            current: account.active,
                            destructive: confirming,
                            ..Default::default()
                        }
                    }));
                }
                rows
            }
            Form::Methods => {
                // connectMethods: key methods sort after interactive ones.
                let mut methods: Vec<_> = self.accounts.methods.iter().enumerate().collect();
                methods.sort_by_key(|(_, method)| matches!(method, AuthMethod::Key));
                methods
                    .into_iter()
                    .map(|(index, method)| plain(index.to_string(), method.label(), ""))
                    .collect()
            }
            Form::CustomAuth => ["API key", "No authentication (explicit)"]
                .iter()
                .enumerate()
                .map(|(index, title)| plain(index.to_string(), title, ""))
                .collect(),
            Form::CustomApi => ["Responses", "Chat Completions", "Messages"]
                .iter()
                .enumerate()
                .map(|(index, title)| plain(index.to_string(), title, ""))
                .collect(),
            _ => Vec::new(),
        };
        let options = Rc::new(options);
        if !self.account_searchable() || self.select.query.trim().is_empty() {
            return options;
        }
        let ranked = self.select.filter_for(options, &TuiPanel::Accounts);
        let mut categories: Vec<&str> = Vec::new();
        for option in ranked.iter() {
            if !categories.contains(&option.category.as_str()) {
                categories.push(&option.category);
            }
        }
        Rc::new(
            categories
                .iter()
                .flat_map(|category| {
                    ranked
                        .iter()
                        .filter(move |option| option.category == *category)
                        .cloned()
                })
                .collect(),
        )
    }

    /// Which pinned surface the Accounts panel currently mounts.
    pub(crate) fn account_surface(&self) -> AccountSurface {
        let view = &self.accounts;
        let masked = || "•".repeat(view.key.chars().count().min(512));
        let refused = |text: &str| {
            if view.failed {
                vec![(text.to_owned(), true)]
            } else {
                Vec::new()
            }
        };
        let notice = |title: String, message: &str| AccountSurface::Notice {
            title,
            link: None,
            instructions: None,
            message: message.into(),
            open_copy: false,
        };
        let name = self.account_provider_name();
        match &view.form {
            Form::Providers if view.pending => {
                notice(crate::dialog::INTEGRATIONS_TITLE.into(), "Working…")
            }
            Form::Providers => AccountSurface::List {
                title: crate::dialog::INTEGRATIONS_TITLE.into(),
                status: if view.failed {
                    vec!["Integrations unavailable".into()]
                } else {
                    Vec::new()
                },
                actions: Vec::new(),
            },
            Form::List if view.pending && view.snapshot.is_none() => notice(name, "Working…"),
            Form::List => AccountSurface::List {
                title: name,
                status: if view.failed {
                    vec!["Account operation refused; secret input cleared".into()]
                } else {
                    Vec::new()
                },
                // manageConnections hides rename/delete on the Add row.
                actions: if self
                    .focused_account(
                        self.modal_options()
                            .get(self.select.cursor)
                            .map(|o| o.value.as_str()),
                    )
                    .is_some()
                {
                    vec![("rename", "ctrl+r".into()), ("delete", "ctrl+d".into())]
                } else {
                    Vec::new()
                },
            },
            Form::Methods if view.methods.is_empty() && !view.failed => {
                notice(format!("Connect {name}"), "Working…")
            }
            Form::Methods => AccountSurface::List {
                title: format!("Connect {name}"),
                status: if view.failed {
                    vec!["No connection method available".into()]
                } else {
                    Vec::new()
                },
                actions: Vec::new(),
            },
            Form::Key => AccountSurface::Prompt {
                title: format!("Connect {name}"),
                description: refused("Account operation refused; secret input cleared"),
                value: masked(),
                placeholder: "API key",
                busy: view.pending,
            },
            Form::Rename(_) => AccountSurface::Prompt {
                title: "Rename account".into(),
                description: Vec::new(),
                value: view.label.clone(),
                placeholder: "Account name",
                busy: view.pending,
            },
            Form::OAuth => {
                let method = view
                    .auth
                    .as_ref()
                    .map_or("OpenAI authorization", |a| a.method.label());
                match &view.attempt {
                    Some(attempt) => AccountSurface::Notice {
                        title: method.into(),
                        link: attempt.url.clone(),
                        instructions: attempt.instructions.clone(),
                        message: if attempt.state == AuthAttemptState::Complete {
                            "Refreshing accounts…"
                        } else if attempt.url.is_some() {
                            "Waiting for authorization…"
                        } else {
                            "Starting authorization…"
                        }
                        .into(),
                        open_copy: self.auth_detail(false).is_some(),
                    },
                    None => notice(method.into(), "Starting authorization…"),
                }
            }
            Form::CustomName => AccountSurface::Prompt {
                title: "Custom provider".into(),
                description: vec![("Display name".into(), false)],
                value: view.custom_name.clone(),
                placeholder: "Display name",
                busy: false,
            },
            Form::CustomBase => AccountSurface::Prompt {
                title: "Custom provider".into(),
                description: vec![("Literal API base URL (prefix is preserved)".into(), false)],
                value: view.custom_base.clone(),
                placeholder: "API base URL",
                busy: false,
            },
            Form::CustomAuth => AccountSurface::List {
                title: "Custom provider".into(),
                status: if view.failed {
                    vec!["Connection operation refused; secret input cleared".into()]
                } else {
                    Vec::new()
                },
                actions: Vec::new(),
            },
            Form::CustomKey => AccountSurface::Prompt {
                title: "Custom provider".into(),
                description: vec![("API key".into(), false)],
                value: masked(),
                placeholder: "API key",
                busy: false,
            },
            Form::CustomProbe => notice("Custom provider".into(), "Checking GET /models…"),
            Form::CustomApi => AccountSurface::List {
                title: "Confirm generation API".into(),
                status: vec![match &view.custom_diagnostic {
                    Some(diagnostic) => format!("Discovery unavailable: {:?}", diagnostic.code),
                    None => format!(
                        "Catalog: {} models (catalog is not generation proof)",
                        view.custom_models
                    ),
                }],
                actions: vec![
                    ("retry GET", "r".into()),
                    ("change authentication", "e".into()),
                ],
            },
            Form::CustomSaving => notice("Custom provider".into(), "Saving connection…"),
        }
    }

    /// The exact rectangle of a non-list surface (prompt/notice) for hit tests.
    fn account_surface_rect(&self, area: Rect) -> Rect {
        match self.account_surface() {
            AccountSurface::Prompt {
                description, busy, ..
            } => crate::dialog::prompt_geometry(area, description.len(), busy),
            AccountSurface::Notice {
                title,
                link,
                instructions,
                message,
                open_copy,
            } => {
                crate::dialog::notice_geometry(
                    area,
                    &crate::dialog::Notice {
                        title: &title,
                        link: link.as_deref(),
                        instructions: instructions.as_deref(),
                        message: &message,
                        open_copy,
                    },
                )
                .0
            }
            AccountSurface::List { .. } => Rect::default(),
        }
    }

    /// Plain-text projection of the mounted surface (tests and text views).
    /// Rendering gets masked glyphs, never a reference to the key.
    pub(crate) fn account_lines(&self) -> Vec<String> {
        match self.account_surface() {
            AccountSurface::List {
                title,
                status,
                actions,
            } => {
                let mut lines = vec![title];
                let mut category = "";
                for (index, option) in self.modal_options().iter().enumerate() {
                    if option.category != category {
                        category = &option.category;
                        lines.push(format!("[{category}]"));
                    }
                    lines.push(format!(
                        "{}{}{}{}{}",
                        if index == self.select.cursor {
                            ">"
                        } else {
                            " "
                        },
                        if option.check {
                            "✓ "
                        } else if option.current {
                            "● "
                        } else {
                            ""
                        },
                        option.title,
                        if option.description.is_empty() {
                            String::new()
                        } else {
                            format!(" {}", option.description)
                        },
                        if option.footer.is_empty() {
                            String::new()
                        } else {
                            format!(" | {}", option.footer)
                        },
                    ));
                }
                lines.extend(status);
                lines.extend(
                    actions
                        .into_iter()
                        .map(|(title, key)| format!("{title} {key}")),
                );
                lines
            }
            AccountSurface::Prompt {
                title,
                description,
                value,
                placeholder,
                busy,
            } => {
                let mut lines = vec![title];
                lines.extend(description.into_iter().map(|(line, _)| line));
                lines.push(if value.is_empty() {
                    placeholder.into()
                } else {
                    value
                });
                lines.push(
                    if busy {
                        "processing…"
                    } else {
                        "enter submit"
                    }
                    .into(),
                );
                lines
            }
            AccountSurface::Notice {
                title,
                link,
                instructions,
                message,
                open_copy,
            } => {
                let mut lines = vec![title];
                lines.extend(link);
                lines.extend(instructions);
                lines.push(message);
                if open_copy {
                    lines.push("o open  c copy".into());
                }
                lines
            }
        }
    }
}

/// The three pinned dialog shapes the Accounts state machine mounts.
pub(crate) enum AccountSurface {
    /// DialogSelect with Search (`modal_options` rows).
    List {
        title: String,
        status: Vec<String>,
        actions: Vec<(&'static str, String)>,
    },
    /// DialogPrompt; `value` is display text (secrets are masked).
    Prompt {
        title: String,
        description: Vec<(String, bool)>,
        value: String,
        placeholder: &'static str,
        busy: bool,
    },
    /// OAuthView / progress view.
    Notice {
        title: String,
        link: Option<String>,
        instructions: Option<String>,
        message: String,
        open_copy: bool,
    },
}

#[cfg(test)]
#[path = "accounts_tests.rs"]
mod tests;
