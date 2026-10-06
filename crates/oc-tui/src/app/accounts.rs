//! One ephemeral secret input, separate from composer, editor undo and copying.
use super::*;
use oc_core::queries::{AccountAction, KeyInput, ProviderAccounts};

#[derive(Default)]
pub(super) struct AccountsView {
    provider: String,
    snapshot: Option<ProviderAccounts>,
    cursor: usize,
    form: Form,
    label: String,
    key: String,
    pending: bool,
    adding: bool,
    confirmation: Option<String>,
    failed: bool,
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
    Label,
    Key,
    Rename(String),
}

impl TuiState {
    pub(super) fn open_accounts(&mut self, connect: bool) -> KeyOutcome {
        self.accounts = AccountsView {
            provider: if connect {
                "opencode-go".into()
            } else {
                self.account_provider()
            },
            form: if connect { Form::Label } else { Form::List },
            ..Default::default()
        };
        self.panel = TuiPanel::Accounts;
        KeyOutcome {
            intent: Some(PanelIntent::ProviderAccounts {
                provider: self.accounts.provider.clone(),
                action: None,
            }),
            consumed_input: true,
            ..Default::default()
        }
    }

    pub(super) fn clear_accounts(&mut self) {
        self.accounts = AccountsView::default();
    }

    /// Owner acknowledgement contains safe metadata only. The submitted key was
    /// moved out of this view before the intent left it, even when ACK fails.
    pub fn apply_provider_accounts(&mut self, result: Result<ProviderAccounts, ()>) -> bool {
        if self.panel != TuiPanel::Accounts {
            return false;
        }
        let adding = self.accounts.adding && self.accounts.pending;
        self.accounts.pending = false;
        self.accounts.adding = false;
        self.accounts.key.clear();
        match result {
            Ok(snapshot) if snapshot.provider == self.accounts.provider => {
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

    pub(super) fn paste_accounts(&mut self, text: &str) -> KeyOutcome {
        if self.accounts.pending {
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
        match (&self.accounts.form, &action) {
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
                self.accounts.form = Form::Label;
                self.accounts.label.clear();
                self.accounts.confirmation = None;
            }
            (Form::List, KeyAction::Up | KeyAction::Down) => {
                let count = self
                    .accounts
                    .snapshot
                    .as_ref()
                    .map_or(0, |s| s.accounts.len());
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
                if let Some(account) = self
                    .accounts
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.accounts.get(self.accounts.cursor))
                {
                    if action == KeyAction::Enter {
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
                        self.accounts.confirmation = None;
                    } else {
                        self.accounts.confirmation = Some(account.id.clone());
                    }
                }
            }
            (_, KeyAction::Char(c)) if self.accounts.form != Form::List && !c.is_control() => {
                return self.paste_accounts(&c.to_string());
            }
            (_, KeyAction::Backspace) if self.accounts.form != Form::List => {
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
                            if index == view.cursor { ">" } else { " " },
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
