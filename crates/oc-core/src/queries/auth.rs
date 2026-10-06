//! Built-in OpenAI authorization projections. No tokens, PKCE verifier or sockets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthMethod {
    Browser,
    Device,
}
impl OAuthMethod {
    pub fn id(self) -> &'static str {
        match self {
            Self::Browser => "chatgpt-browser",
            Self::Device => "chatgpt-headless",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Browser => "ChatGPT Pro/Plus (browser)",
            Self::Device => "ChatGPT Pro/Plus (headless)",
        }
    }
}

/// Selectable methods only: inherited environment credentials are not an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    OAuth(OAuthMethod),
    Key,
}
impl AuthMethod {
    pub fn id(self) -> &'static str {
        match self {
            Self::OAuth(method) => method.id(),
            Self::Key => "key",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::OAuth(method) => method.label(),
            Self::Key => "API key",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthAttemptFailure {
    Cancelled,
    Callback,
    StateMismatch,
    Remote,
    InvalidTokens,
    AccountChanged,
    Unavailable,
}
impl AuthAttemptFailure {
    pub fn message(self) -> &'static str {
        match self {
            Self::Cancelled => "Authorization cancelled",
            Self::Callback => "Authorization callback rejected",
            Self::StateMismatch => "Authorization state mismatch",
            Self::Remote => "Authorization request failed",
            Self::InvalidTokens => "Authorization token response is invalid",
            Self::AccountChanged => "Account selection changed during authorization",
            Self::Unavailable => "Authorization unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthAttemptState {
    Pending,
    Complete,
    Failed(AuthAttemptFailure),
    Expired,
}

/// URL/state and device instructions are presented only by explicit login UI.
/// Debug is redacted even though these are not stored access/refresh tokens.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthAttempt {
    pub id: String,
    pub method: OAuthMethod,
    pub state: AuthAttemptState,
    pub url: Option<String>,
    pub instructions: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
    /// Local SQLite account ID, not ChatGPT routing metadata.
    pub account_id: Option<String>,
}
impl std::fmt::Debug for AuthAttempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthAttempt")
            .field("id", &self.id)
            .field("method", &self.method)
            .field("state", &self.state)
            .field("authorization", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthAction {
    Begin { method: OAuthMethod, label: String },
    Status { attempt: String },
    Cancel { attempt: String },
}
