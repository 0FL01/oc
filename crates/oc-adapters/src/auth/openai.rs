//! Built-in OpenAI credentials: one SQLite owner, scoped captures and refresh.
//! Login attempts and transport consume this owner; neither can fall back after
//! an active OAuth account becomes unavailable.
use super::{AuthError, AuthPolicy, AuthScope, AuthSource, ResolvedAuth};
use crate::storage::{CredentialMaterial, Db, OAuthAccountMetadata};
use base64::Engine;
use serde::Deserialize;
use sha2::Digest;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub(super) const ISSUER: &str = "https://auth.openai.com";
pub(super) const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub(super) const BROWSER: &str = "chatgpt-browser";
pub(super) const DEVICE: &str = "chatgpt-headless";
const TOKEN_BODY_BYTES: usize = 64 * 1024;

fn key_scope(namespace: &str, source: AuthSource, key: Option<&str>) -> Result<String, AuthError> {
    let fingerprint = key.map(|key| {
        sha2::Sha256::digest(key.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    });
    serde_json::to_string(&(namespace, format!("{source:?}"), fingerprint))
        .map_err(|_| AuthError::Authority)
}

/// Secret-bearing response, intentionally without Debug. Metadata is trusted
/// only because it came from this admitted token exchange, not a JWT signature.
#[derive(Deserialize)]
pub(super) struct Tokens {
    id_token: String,
    access_token: String,
    refresh_token: String,
    expires_in: Option<i64>,
}

#[derive(Deserialize)]
struct Claims {
    chatgpt_account_id: Option<String>,
    #[serde(rename = "https://api.openai.com/auth")]
    auth: Option<AccountClaim>,
    organizations: Option<Vec<Organization>>,
}
#[derive(Deserialize)]
struct AccountClaim {
    chatgpt_account_id: Option<String>,
}
#[derive(Deserialize)]
struct Organization {
    id: String,
}

fn claim(token: &str) -> Option<OAuthAccountMetadata> {
    let part = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(part)
        .ok()?;
    let claims: Claims = serde_json::from_slice(&bytes).ok()?;
    let account_id = claims
        .chatgpt_account_id
        .or_else(|| claims.auth.and_then(|auth| auth.chatgpt_account_id))
        .or_else(|| {
            claims
                .organizations
                .and_then(|orgs| orgs.into_iter().next().map(|org| org.id))
        })?;
    Some(OAuthAccountMetadata { account_id })
}

impl Tokens {
    pub(super) fn material(self, method: &str, now: i64) -> Result<CredentialMaterial, AuthError> {
        if !matches!(method, BROWSER | DEVICE) || self.id_token.len() > 16 * 1024 {
            return Err(AuthError::InvalidTokens);
        }
        let metadata = claim(&self.id_token).or_else(|| claim(&self.access_token));
        let lifetime = self.expires_in.unwrap_or(3600);
        let expires_at = now
            .checked_add(lifetime)
            .filter(|_| lifetime >= 0)
            .ok_or(AuthError::InvalidTokens)?;
        let material = CredentialMaterial::OAuth {
            access: self.access_token,
            refresh: Some(self.refresh_token),
            expires_at: Some(expires_at),
            method_id: Some(method.into()),
            metadata,
        };
        if !material.valid() {
            return Err(AuthError::InvalidTokens);
        }
        Ok(material)
    }
}

/// Native bounded token HTTP client. Endpoint overrides are private test fixtures;
/// production always uses the pinned issuer and disables redirects and retries.
pub struct OpenAiAuth {
    pub(super) client: reqwest::Client,
    pub(super) issuer: String,
}

impl OpenAiAuth {
    pub fn new() -> Result<Self, AuthError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(format!("opencode/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| AuthError::Remote)?;
        Ok(Self {
            client,
            issuer: ISSUER.into(),
        })
    }

    pub(super) async fn tokens(&self, fields: &[(&str, &str)]) -> Result<Tokens, AuthError> {
        // Use the existing URL form serializer; reqwest's optional `form` feature
        // is not enabled. This URL is only a bounded body encoder, never dialled.
        let mut form = reqwest::Url::parse(&self.issuer).map_err(|_| AuthError::Authority)?;
        form.query_pairs_mut().extend_pairs(fields.iter().copied());
        let mut response = self
            .client
            .post(format!("{}/oauth/token", self.issuer))
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(form.query().unwrap_or_default().to_owned())
            .send()
            .await
            .map_err(|_| AuthError::Remote)?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|n| n > TOKEN_BODY_BYTES as u64)
        {
            return Err(AuthError::Remote);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| AuthError::Remote)? {
            if bytes.len().saturating_add(chunk.len()) > TOKEN_BODY_BYTES {
                return Err(AuthError::InvalidTokens);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| AuthError::InvalidTokens)
    }

    /// Active stored account wins over own environment/config. An OAuth failure
    /// is explicit, never another account, API key or anonymous fallback.
    pub async fn resolve(
        &self,
        db: &Db,
        policy: AuthPolicy,
        configured: Option<&str>,
        environment: impl FnOnce() -> Option<String>,
    ) -> Result<ResolvedAuth, AuthError> {
        self.resolve_at(db, policy, configured, environment, || {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64)
        })
        .await
    }

    async fn resolve_at(
        &self,
        db: &Db,
        policy: AuthPolicy,
        configured: Option<&str>,
        environment: impl FnOnce() -> Option<String>,
        now: impl Fn() -> i64,
    ) -> Result<ResolvedAuth, AuthError> {
        if policy == AuthPolicy::None {
            return Err(AuthError::Conflict);
        }
        let scope = AuthScope::admit("openai", OPENAI_BASE_URL)?;
        let Some(captured) = db.credential_snapshot(scope.namespace())? else {
            if policy == AuthPolicy::OAuth {
                return Err(AuthError::Reauthenticate);
            }
            let env = environment().filter(|key| !key.trim().is_empty());
            let source = if env.is_some() {
                AuthSource::OpenAiEnvironment
            } else if configured.is_some_and(|v| !v.trim().is_empty()) {
                AuthSource::Configured
            } else {
                AuthSource::Missing
            };
            let key = env.or_else(|| {
                configured
                    .filter(|v| !v.trim().is_empty())
                    .map(String::from)
            });
            return Ok(ResolvedAuth {
                source,
                namespace: key_scope(scope.namespace(), source, key.as_deref())?,
                key,
                oauth: None,
            });
        };
        if let CredentialMaterial::Key { key } = &captured.material {
            if policy == AuthPolicy::OAuth {
                return Err(AuthError::Reauthenticate);
            }
            return Ok(ResolvedAuth {
                source: AuthSource::Stored,
                namespace: serde_json::to_string(&(scope.namespace(), &captured.id, "key"))
                    .map_err(|_| AuthError::Authority)?,
                key: Some(key.clone()),
                oauth: None,
            });
        }
        let _flight = db.credential_refresh.lock().await;
        let mut current = db
            .credential_snapshot(scope.namespace())?
            .ok_or(AuthError::StaleCredential)?;
        if current.id != captured.id || current.selection_revision != captured.selection_revision {
            return Err(AuthError::StaleCredential);
        }
        let CredentialMaterial::OAuth {
            refresh,
            expires_at,
            method_id,
            metadata,
            ..
        } = &current.material
        else {
            return Err(AuthError::StaleCredential);
        };
        let method = method_id
            .as_deref()
            .filter(|method| matches!(*method, BROWSER | DEVICE))
            .ok_or(AuthError::UnsupportedMethod)?;
        let expiry = expires_at.ok_or(AuthError::Reauthenticate)?;
        if current.refresh_pending {
            return Err(AuthError::Reauthenticate);
        }
        if expiry <= now().saturating_add(300) {
            let refresh = refresh.as_deref().ok_or(AuthError::Reauthenticate)?;
            if !db.begin_credential_refresh(&current)? {
                return Err(AuthError::StaleCredential);
            }
            let mut next = self
                .tokens(&[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh),
                    ("client_id", CLIENT_ID),
                ])
                .await?
                .material(method, now())?;
            if let CredentialMaterial::OAuth {
                metadata: next_metadata,
                ..
            } = &mut next
                && next_metadata.is_none()
            {
                *next_metadata = metadata.clone();
            }
            if !db.rotate_credential(&current, next)? {
                return Err(AuthError::StaleCredential);
            }
            current = db
                .credential_snapshot(scope.namespace())?
                .ok_or(AuthError::StaleCredential)?;
        }
        if current.id != captured.id || current.selection_revision != captured.selection_revision {
            return Err(AuthError::StaleCredential);
        }
        let CredentialMaterial::OAuth {
            access,
            method_id,
            metadata,
            ..
        } = current.material
        else {
            return Err(AuthError::StaleCredential);
        };
        Ok(ResolvedAuth {
            source: AuthSource::StoredOAuth,
            namespace: serde_json::to_string(&(
                scope.namespace(),
                current.id,
                "oauth",
                method_id,
                metadata.as_ref().map(|m| &m.account_id),
            ))
            .map_err(|_| AuthError::Authority)?,
            key: Some(access),
            oauth: metadata,
        })
    }
}

#[cfg(test)]
mod tests;
