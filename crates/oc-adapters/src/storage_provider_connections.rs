//! Typed, bounded connection metadata in the existing locked database. No keys here.
use super::*;
use crate::auth::{AuthPolicy, AuthScope};
use oc_core::queries::{CustomProviderAuth, CustomProviderInput, NativeProtocol};
use std::collections::BTreeMap;

#[cfg(test)]
#[path = "storage_provider_connections/tests.rs"]
mod tests;

pub(crate) const MAX_CONNECTIONS: usize = 128;
pub(crate) const MAX_CATALOG_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_MODELS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CatalogState {
    Missing,
    Ready,
    Failed,
    Unauthorized,
    Forbidden,
}

/// Only sanitized discovery metadata is admitted to `models` by the catalog owner.
#[derive(Clone)]
pub(crate) struct CustomConnection {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) base_url: String,
    pub(crate) protocol: NativeProtocol,
    pub(crate) auth: AuthPolicy,
    pub(crate) revision: i64,
    pub(crate) catalog_revision: Option<i64>,
    pub(crate) models: BTreeMap<String, serde_json::Value>,
    pub(crate) state: CatalogState,
}

impl std::fmt::Debug for CustomConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomConnection")
            .field("id", &self.id)
            .field("protocol", &self.protocol)
            .field("revision", &self.revision)
            .field("models", &self.models.len())
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

/// Literal user authority: no environment/file expansion, host scan or /v1 insertion.
pub(crate) fn admit_input(input: &CustomProviderInput) -> Result<(), StorageError> {
    if !credentials::metadata(&input.name, 128)
        || input.base_url.trim() != input.base_url
        || input.base_url.len() > 4096
        || input
            .base_url
            .chars()
            .any(|c| c.is_control() || matches!(c, '{' | '}'))
        || crate::endpoint::EndpointBinding::admit(&input.base_url, true, "custom connection")
            .is_err()
    {
        return Err(StorageError::InvalidCredential);
    }
    if let CustomProviderAuth::Key(key) = &input.auth
        && !(CredentialMaterial::Key {
            key: key.clone().into_secret(),
        })
        .valid()
    {
        return Err(StorageError::InvalidCredential);
    }
    Ok(())
}

fn encode_models(models: &BTreeMap<String, serde_json::Value>) -> Result<String, StorageError> {
    if models.len() > MAX_MODELS || models.keys().any(|id| !credentials::metadata(id, 4096)) {
        return Err(StorageError::InvalidCredential);
    }
    let json = serde_json::to_string(models).map_err(|_| StorageError::CredentialStorage)?;
    if json.len() > MAX_CATALOG_BYTES {
        return Err(StorageError::InvalidCredential);
    }
    if crate::custom_discovery::sanitize_snapshot(models)
        .map_err(|_| StorageError::InvalidCredential)?
        != *models
    {
        return Err(StorageError::InvalidCredential);
    }
    Ok(json)
}

impl Db {
    pub(super) fn custom_provider_schema(conn: &Connection) -> Result<(), StorageError> {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS custom_provider_connections(
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            base_url TEXT NOT NULL,
            protocol TEXT NOT NULL CHECK(protocol IN ('responses','chat','messages')),
            auth_policy TEXT NOT NULL CHECK(auth_policy IN ('key','none')),
            auth_namespace TEXT NOT NULL UNIQUE,
            binding_revision INTEGER NOT NULL CHECK(typeof(binding_revision)='integer' AND binding_revision>0),
            catalog_revision INTEGER,
            models_json TEXT NOT NULL CHECK(json_valid(models_json) AND length(CAST(models_json AS BLOB))<=8388608),
            catalog_state TEXT NOT NULL CHECK(catalog_state IN ('missing','ready','failed','unauthorized','forbidden')));
            INSERT OR IGNORE INTO schema_migrations(version,applied_at) VALUES(14,'t59-custom-connections');")?;
        Ok(())
    }

    /// Credential mutations invalidate cached binding provenance in their own transaction.
    pub(super) fn invalidate_custom_binding(
        tx: &rusqlite::Transaction<'_>,
        namespace: &str,
    ) -> Result<(), StorageError> {
        tx.execute("UPDATE custom_provider_connections SET binding_revision=binding_revision+1,catalog_state='missing' WHERE auth_namespace=?1", [namespace])
            .map_err(credentials::credential_error)?;
        Ok(())
    }

    pub(crate) fn create_custom_provider(
        &self,
        input: &CustomProviderInput,
        protocol: NativeProtocol,
        admitted: impl FnOnce(&str) -> bool,
    ) -> Result<CustomConnection, StorageError> {
        admit_input(input)?;
        let mut conn = self.conn.lock().expect("db mutex");
        let mut epochs = self.credential_epochs.lock().expect("credential epochs");
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(credentials::credential_error)?;
        let count: i64 = tx
            .query_row(
                "SELECT count(*) FROM custom_provider_connections",
                [],
                |r| r.get(0),
            )
            .map_err(credentials::credential_error)?;
        if count >= MAX_CONNECTIONS as i64 {
            return Err(StorageError::CredentialStorage);
        }
        let id: String = tx
            .query_row("SELECT 'custom-' || lower(hex(randomblob(16)))", [], |r| {
                r.get(0)
            })
            .map_err(credentials::credential_error)?;
        if !admitted(&id) {
            return Err(StorageError::InvalidCredential);
        }
        let scope =
            AuthScope::custom(&id, &input.base_url).map_err(|_| StorageError::InvalidCredential)?;
        let auth = match &input.auth {
            CustomProviderAuth::Key(_) => AuthPolicy::Key,
            CustomProviderAuth::Anonymous => AuthPolicy::None,
        };
        let protocol_json =
            serde_json::to_value(protocol).map_err(|_| StorageError::InvalidCredential)?;
        let auth_json = serde_json::to_value(auth).map_err(|_| StorageError::InvalidCredential)?;
        tx.execute("INSERT INTO custom_provider_connections(id,name,base_url,protocol,auth_policy,auth_namespace,binding_revision,models_json,catalog_state) VALUES(?1,?2,?3,?4,?5,?6,1,'{}','missing')",
            params![id,input.name,input.base_url,protocol_json.as_str(),auth_json.as_str(),scope.namespace()])
            .map_err(credentials::credential_error)?;
        let next_epoch = epochs
            .get(scope.namespace())
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(StorageError::CredentialStorage)?;
        if let CustomProviderAuth::Key(key) = &input.auth {
            let material = CredentialMaterial::Key {
                key: key.clone().into_secret(),
            };
            let raw =
                serde_json::to_string(&material).map_err(|_| StorageError::InvalidCredential)?;
            credentials::insert_credential(&tx, scope.namespace(), "default", &material, &raw)?;
        }
        tx.commit().map_err(credentials::credential_error)?;
        if auth == AuthPolicy::Key {
            epochs.insert(scope.namespace().into(), next_epoch);
        }
        Ok(CustomConnection {
            id,
            name: input.name.clone(),
            base_url: input.base_url.clone(),
            protocol,
            auth,
            revision: 1,
            catalog_revision: None,
            models: BTreeMap::new(),
            state: CatalogState::Missing,
        })
    }

    /// Complete-success replacement, including valid empty. Failure retains last-good IDs.
    pub(crate) fn save_custom_catalog(
        &self,
        id: &str,
        revision: i64,
        models: &BTreeMap<String, serde_json::Value>,
        state: CatalogState,
    ) -> Result<bool, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let changed = if state == CatalogState::Ready {
            let json = encode_models(models)?;
            conn.execute("UPDATE custom_provider_connections SET models_json=?3,catalog_revision=?2,catalog_state='ready' WHERE id=?1 AND binding_revision=?2", params![id,revision,json])
        } else {
            conn.execute("UPDATE custom_provider_connections SET catalog_state=?3 WHERE id=?1 AND binding_revision=?2", params![id,revision,serde_json::to_value(state).map_err(|_| StorageError::CredentialStorage)?.as_str()])
        }.map_err(credentials::credential_error)?;
        Ok(changed == 1)
    }

    pub(crate) fn custom_providers(&self) -> Result<Vec<CustomConnection>, StorageError> {
        read_connections(&self.conn.lock().expect("db mutex"))
    }

    /// Catalog listing never acquires the data-root write lock or reads credential material.
    pub(crate) fn saved_custom_providers(
        root: &Path,
    ) -> Result<Vec<CustomConnection>, StorageError> {
        let Some(conn) = Self::public_read_only_connection(root) else {
            return if root.join("oc.sqlite").exists() {
                Err(StorageError::CredentialStorage)
            } else {
                Ok(Vec::new())
            };
        };
        read_connections(&conn)
    }
}

fn read_connections(conn: &Connection) -> Result<Vec<CustomConnection>, StorageError> {
    let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='custom_provider_connections')", [], |r| r.get(0)).map_err(credentials::credential_error)?;
    if !exists {
        return Ok(Vec::new());
    }
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM custom_provider_connections",
            [],
            |r| r.get(0),
        )
        .map_err(credentials::credential_error)?;
    if count > MAX_CONNECTIONS as i64 {
        return Err(StorageError::CredentialStorage);
    }
    let mut statement = conn.prepare("SELECT id,name,base_url,protocol,auth_policy,binding_revision,catalog_revision,models_json,catalog_state FROM custom_provider_connections ORDER BY id").map_err(credentials::credential_error)?;
    let rows = statement
        .query_map([], |r| {
            let decode = |index| -> rusqlite::Result<serde_json::Value> {
                Ok(serde_json::Value::String(r.get(index)?))
            };
            let raw = r.get_ref(7)?.as_str()?;
            if raw.len() > MAX_CATALOG_BYTES {
                return Err(rusqlite::Error::InvalidQuery);
            }
            let models: BTreeMap<String, serde_json::Value> =
                serde_json::from_str(raw).map_err(|_| rusqlite::Error::InvalidQuery)?;
            if encode_models(&models).is_err() {
                return Err(rusqlite::Error::InvalidQuery);
            }
            let bounded = |index, max| -> rusqlite::Result<String> {
                let value = r.get_ref(index)?.as_str()?;
                if value.len() > max {
                    return Err(rusqlite::Error::InvalidQuery);
                }
                Ok(value.into())
            };
            let id = bounded(0, 39)?;
            if id.len() != 39
                || !id.strip_prefix("custom-").is_some_and(|value| {
                    value
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
            {
                return Err(rusqlite::Error::InvalidQuery);
            }
            let name = bounded(1, 128)?;
            let base_url = bounded(2, 4096)?;
            if !credentials::metadata(&name, 128)
                || crate::endpoint::EndpointBinding::admit(
                    &base_url,
                    true,
                    "saved custom connection",
                )
                .is_err()
            {
                return Err(rusqlite::Error::InvalidQuery);
            }
            Ok(CustomConnection {
                id,
                name,
                base_url,
                protocol: serde_json::from_value(decode(3)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                auth: serde_json::from_value(decode(4)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                revision: r.get(5)?,
                catalog_revision: r.get(6)?,
                models,
                state: serde_json::from_value(decode(8)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
            })
        })
        .map_err(credentials::credential_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(credentials::credential_error)
}
