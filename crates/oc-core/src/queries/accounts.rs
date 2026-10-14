//! Account commands are typed owner mutations, never composer input.

/// Admitted connection identity only; never endpoint, headers or credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConnection {
    pub provider: String,
    pub name: String,
}

/// Ephemeral key input. Deliberately not serializable and always redacted.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyInput(String);
impl KeyInput {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn into_secret(self) -> String {
        self.0
    }
}
impl std::fmt::Debug for KeyInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeyInput([REDACTED])")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountKind {
    Key,
    OAuth,
}

/// No token preview, length, endpoint, or secret-bearing namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAccount {
    pub id: String,
    pub label: String,
    pub kind: AccountKind,
    /// Known OAuth method identity, never routing metadata or token contents.
    pub method_id: Option<String>,
    pub active: bool,
    pub created_at: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAccounts {
    pub provider: String,
    pub accounts: Vec<ProviderAccount>,
    pub effective: AccountAuthSource,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountAuthSource {
    Stored,
    Environment,
    Configured,
    Anonymous,
    UnsupportedOAuth,
    Missing,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountAction {
    AddKey { label: String, key: KeyInput },
    Activate { id: String },
    Rename { id: String, label: String },
    Remove { id: String, confirmed: bool },
}

/// Explicit endpoint onboarding, not configuration text or an executable extension.
#[derive(Clone, PartialEq, Eq)]
pub struct CustomProviderInput {
    pub name: String,
    pub base_url: String,
    pub auth: CustomProviderAuth,
}

impl std::fmt::Debug for CustomProviderInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomProviderInput")
            .field("endpoint", &"[user supplied]")
            .field("auth", &self.auth)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomProviderAuth {
    Key(KeyInput),
    Anonymous,
}

/// Probe work is cancellable and has no persistent effect. Save consumes its exact receipt.
#[derive(Debug, Clone)]
pub enum CustomProviderAction {
    Probe {
        input: CustomProviderInput,
        cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    },
    Save {
        revision: u64,
        protocol: super::NativeProtocol,
    },
    Cancel {
        revision: u64,
    },
    Refresh {
        provider: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomProviderReply {
    Probed {
        revision: u64,
        model_count: usize,
        suggested: Option<super::NativeProtocol>,
        diagnostic: Option<super::ServiceDiagnostic>,
    },
    Saved(ProviderConnection),
    Cancelled,
    Refreshing,
}
