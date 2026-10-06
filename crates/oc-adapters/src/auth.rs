//! Narrow provider-owned credential resolution. Admission precedes all secret reads.
use crate::storage::{CredentialMaterial, Db, StorageError};
use serde::{Deserialize, Serialize};

pub const GO_BASE_URL: &str = "https://opencode.ai/zen/go/v1";

mod openai;
pub use openai::{OPENAI_BASE_URL, OpenAiAuth};

/// Omitted policy retains legacy required Key behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthPolicy {
    None,
    #[default]
    Key,
    #[serde(rename = "oauth")]
    OAuth,
}

/// Safe origin of the selected credential; contains no material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthSource {
    Anonymous,
    Configured,
    Stored,
    StoredOAuth,
    GoEnvironment,
    OpenAiEnvironment,
    Missing,
    UnsupportedOAuth,
}

/// Captured resolved auth. Debug is intentionally metadata-only.
#[derive(Clone)]
pub struct ResolvedAuth {
    pub source: AuthSource,
    pub namespace: String,
    pub(crate) key: Option<String>,
    pub(crate) oauth: Option<crate::storage::OAuthAccountMetadata>,
}

impl std::fmt::Debug for ResolvedAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedAuth")
            .field("source", &self.source)
            .field("oauth_metadata", &self.oauth.is_some())
            .finish_non_exhaustive()
    }
}

impl ResolvedAuth {
    /// Install the immutable resolver result into a captured transport config.
    pub fn apply_to(self, provider: &mut crate::provider::ResponsesConfig) {
        provider.wire.auth_policy = match self.source {
            AuthSource::Anonymous => AuthPolicy::None,
            AuthSource::UnsupportedOAuth | AuthSource::StoredOAuth => AuthPolicy::OAuth,
            _ => AuthPolicy::Key,
        };
        provider.api_key = self.key.unwrap_or_default();
    }
}

/// Admitted normalized endpoint/auth scope. Construction performs authority checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthScope {
    namespace: String,
    go: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("provider endpoint authority rejected")]
    Authority,
    #[error("conflicting authentication inputs")]
    Conflict,
    #[error("OpenAI OAuth method is unsupported; choose an admitted login method")]
    UnsupportedMethod,
    #[error("OpenAI credential changed during resolution")]
    StaleCredential,
    #[error("OpenAI OAuth unavailable; explicit reauthentication required")]
    Reauthenticate,
    #[error("OpenAI token response is invalid")]
    InvalidTokens,
    #[error("OpenAI authorization request failed")]
    Remote,
    #[error("credential storage unavailable")]
    Storage(#[from] StorageError),
}

impl AuthScope {
    /// Must be called only for the effective locally admitted connection, not remote metadata.
    pub fn admit(provider: &str, base_url: &str) -> Result<Self, AuthError> {
        let mut url = reqwest::Url::parse(base_url).map_err(|_| AuthError::Authority)?;
        if provider.is_empty()
            || !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(AuthError::Authority);
        }
        let path = url.path().trim_end_matches('/').to_owned();
        url.set_path(&path);
        let go = provider == "opencode-go";
        if go && url.as_str() != GO_BASE_URL {
            return Err(AuthError::Authority);
        }
        // JSON tuple avoids delimiter collisions in provider IDs/path prefixes.
        let namespace =
            serde_json::to_string(&(provider, url.as_str())).map_err(|_| AuthError::Authority)?;
        Ok(Self { namespace, go })
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Env is lazy: foreign/rejected endpoints never cause OPENCODE_API_KEY reads.
    pub fn resolve(
        &self,
        db: &Db,
        policy: AuthPolicy,
        configured: Option<&str>,
        competing_auth: bool,
        go_env: impl FnOnce() -> Option<String>,
    ) -> Result<ResolvedAuth, AuthError> {
        let configured = configured.filter(|key| !key.trim().is_empty());
        if policy == AuthPolicy::None {
            if configured.is_some() || competing_auth {
                return Err(AuthError::Conflict);
            }
            return Ok(ResolvedAuth {
                source: AuthSource::Anonymous,
                namespace: self.namespace.clone(),
                key: None,
                oauth: None,
            });
        }
        let result = |source, key| ResolvedAuth {
            source,
            namespace: self.namespace.clone(),
            key,
            oauth: None,
        };
        if policy == AuthPolicy::OAuth {
            return Ok(result(AuthSource::UnsupportedOAuth, None));
        }
        if !self.go
            && let Some(key) = configured
        {
            return Ok(result(AuthSource::Configured, Some(key.into())));
        }
        if let Some(material) = db.active_credential(&self.namespace)? {
            return Ok(match material {
                CredentialMaterial::Key { key } => result(AuthSource::Stored, Some(key)),
                CredentialMaterial::OAuth { .. } => result(AuthSource::UnsupportedOAuth, None),
            });
        }
        if self.go
            && let Some(key) = go_env().filter(|key| !key.trim().is_empty())
        {
            return Ok(result(AuthSource::GoEnvironment, Some(key)));
        }
        Ok(match configured {
            Some(key) => result(AuthSource::Configured, Some(key.into())),
            None => result(AuthSource::Missing, None),
        })
    }
}
