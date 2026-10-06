//! One ephemeral secret input, separate from composer, editor undo and copying.
use super::*;
use oc_core::queries::{
    AccountAction, AuthAttempt, AuthAttemptState, AuthMethod, KeyInput, OAuthMethod,
    ProviderAccounts, ProviderConnection,
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
    Label,
    Key,
    Methods,
    OAuth,
    Rename(String),
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
            ..Default::default()
        };
        self.panel = TuiPanel::Accounts;
        KeyOutcome {
            intent: Some(if connect {
                PanelIntent::LoadProviderConnections
            } else {
                PanelIntent::ProviderAccounts {
                    provider: self.accounts.provider.clone(),
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

    pub fn apply_provider_connections(&mut self, result: Result<Vec<ProviderConnection>, ()>) {
        if self.panel != TuiPanel::Accounts || self.accounts.form != Form::Providers {
            return;
        }
        self.accounts.connections = result.unwrap_or_default();
        self.accounts.cursor = 0;
        self.accounts.failed = self.accounts.connections.is_empty();
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
                if self.accounts.connecting && snapshot.provider == "openai" {
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
                if adding {
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
        if self.accounts.methods.as_slice() == [AuthMethod::Key] {
            self.accounts.form = Form::Label;
        }
    }

    pub fn auth_request(&self) -> Option<&AuthRequest> {
        (self.panel == TuiPanel::Accounts && self.accounts.form == Form::OAuth)
            .then_some(self.accounts.auth.as_ref())
            .flatten()
    }

    /// A parked view is unmounted: discard its active URL/code immediately.
    pub fn retire_auth_surface(&mut self) {
        if self.auth_request().is_some() {
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
                    let count = self.accounts.connections.len();
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
                    let add = usize::from(self.accounts.provider == "openai");
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
                    Form::List if self.accounts.provider == "openai" && row == 0 => 0,
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
        self.accounts.form = if self.accounts.provider == "openai" {
            Form::Methods
        } else {
            Form::Label
        };
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
        if self.accounts.pending
            || !matches!(
                self.accounts.form,
                Form::Label | Form::Key | Form::Rename(_)
            )
        {
            return KeyOutcome::default();
        }
        let value = if self.accounts.form == Form::Key {
            &mut self.accounts.key
        } else if self.accounts.form != Form::List {
            &mut self.accounts.label
        } else {
            return KeyOutcome::default();
        };
        let cap = if self.accounts.form == Form::Key {
            16 * 1024
        } else {
            128
        };
        if !text.chars().any(char::is_control) && value.len().saturating_add(text.len()) <= cap {
            value.push_str(text);
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
            (Form::List | Form::Providers | Form::Methods, KeyAction::Char('j')) => KeyAction::Down,
            (Form::List | Form::Providers | Form::Methods, KeyAction::Char('k')) => KeyAction::Up,
            (_, action) => action,
        };
        match (&self.accounts.form, &action) {
            (Form::Providers, KeyAction::Up | KeyAction::Down) => {
                self.accounts.cursor = self
                    .accounts
                    .cursor
                    .saturating_add_signed(if action == KeyAction::Up { -1 } else { 1 })
                    .min(self.accounts.connections.len().saturating_sub(1));
            }
            (Form::Providers, KeyAction::Enter) => {
                if let Some(connection) = self.accounts.connections.get(self.accounts.cursor) {
                    self.accounts.provider = connection.provider.clone();
                    self.accounts.form = Form::Label;
                    self.accounts.cursor = 0;
                    return KeyOutcome {
                        intent: Some(PanelIntent::ProviderAccounts {
                            provider: self.accounts.provider.clone(),
                            action: None,
                        }),
                        ..Default::default()
                    };
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
                    Some(AuthMethod::Key) => self.accounts.form = Form::Label,
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
            (Form::Label, KeyAction::Enter) if !self.accounts.label.trim().is_empty() => {
                self.accounts.form = Form::Key;
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
                    + usize::from(self.accounts.provider == "openai");
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
                if self.accounts.provider == "openai" && self.accounts.cursor == 0 {
                    if action == KeyAction::Enter {
                        return self.choose_account_method();
                    }
                    return KeyOutcome::default();
                }
                if let Some(account) = self.accounts.snapshot.as_ref().and_then(|s| {
                    s.accounts
                        .get(self.accounts.cursor - usize::from(self.accounts.provider == "openai"))
                }) {
                    if action == KeyAction::Enter {
                        if self.accounts.provider == "openai" && account.active {
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
            (_, KeyAction::Char(c))
                if matches!(
                    self.accounts.form,
                    Form::Label | Form::Key | Form::Rename(_)
                ) && !c.is_control() =>
            {
                return self.paste_accounts(&c.to_string());
            }
            (_, KeyAction::Backspace)
                if matches!(
                    self.accounts.form,
                    Form::Label | Form::Key | Form::Rename(_)
                ) =>
            {
                if self.accounts.form == Form::Key {
                    self.accounts.key.pop();
                } else {
                    self.accounts.label.pop();
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
                action: Some(action),
            }),
            ..Default::default()
        }
    }

    /// Rendering gets masked glyphs, never a reference to the key.
    pub(crate) fn account_lines(&self) -> Vec<String> {
        let view = &self.accounts;
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
            lines.push("Saving…".into());
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
                lines.push("Choose provider connection".into());
                for (index, connection) in view
                    .connections
                    .iter()
                    .enumerate()
                    .skip(view.cursor.saturating_sub(3))
                    .take(8)
                {
                    lines.push(format!(
                        "{} {} ({})",
                        if index == view.cursor { ">" } else { " " },
                        connection.name,
                        connection.provider
                    ));
                }
            }
            Form::Label => {
                lines.push("Account label".into());
                lines.push(view.label.clone());
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
                if view.provider == "openai" {
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
                            if index + usize::from(view.provider == "openai") == view.cursor {
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
        }
        lines.push("enter continue · esc cancel".into());
        lines
    }
}

#[cfg(test)]
#[path = "accounts_tests.rs"]
mod tests;
