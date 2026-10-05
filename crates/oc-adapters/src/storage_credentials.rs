//! Account material lives only in the existing locked SQLite owner.
use super::*;

#[cfg(test)]
#[path = "storage_credentials/tests.rs"]
mod tests;

/// Persisted credential kind; None is a connection policy, never an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    /// Static API key.
    Key,
    /// Stored OAuth tokens, not an implemented OAuth execution flow.
    #[serde(rename = "oauth")]
    OAuth,
}

/// Secret-bearing material. Debug never exposes token contents.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CredentialMaterial {
    /// Static key.
    Key { key: String },
    /// OAuth storage roundtrip only; resolver must explicitly support execution.
    #[serde(rename = "oauth")]
    OAuth {
        access: String,
        refresh: Option<String>,
        expires_at: Option<i64>,
    },
}

impl std::fmt::Debug for CredentialMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialMaterial")
            .field("kind", &self.kind())
            .field("material", &"[REDACTED]")
            .finish()
    }
}

impl CredentialMaterial {
    /// Safe kind for UI/readiness, without copying secret material.
    pub fn kind(&self) -> CredentialKind {
        match self {
            Self::Key { .. } => CredentialKind::Key,
            Self::OAuth { .. } => CredentialKind::OAuth,
        }
    }

    fn valid(&self) -> bool {
        fn token(value: &str) -> bool {
            !value.trim().is_empty()
                && value.len() <= 16 * 1024
                && !value.chars().any(char::is_control)
        }
        match self {
            Self::Key { key } => token(key),
            Self::OAuth {
                access,
                refresh,
                expires_at,
            } => {
                token(access)
                    && refresh.as_deref().is_none_or(token)
                    && expires_at.is_none_or(|expiry| expiry >= 0)
            }
        }
    }
}

/// Public account projection contains no material, previews, or token lengths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSummary {
    pub id: String,
    pub provider_namespace: String,
    pub label: String,
    pub kind: CredentialKind,
    pub active: bool,
    pub created_at: i64,
}

fn metadata(value: &str, limit: usize) -> bool {
    !value.trim().is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}

fn credential_error(_: rusqlite::Error) -> StorageError {
    StorageError::CredentialStorage
}

impl Db {
    pub(super) fn credentials_schema(conn: &Connection) -> Result<(), StorageError> {
        let tx =
            rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
        tx.execute_batch("
            CREATE TABLE IF NOT EXISTS credential_accounts(
                id TEXT PRIMARY KEY,
                provider_namespace TEXT NOT NULL,
                label TEXT NOT NULL,
                tagged_value_json TEXT NOT NULL CHECK(json_valid(tagged_value_json)),
                active INTEGER NOT NULL CHECK(active IN (0,1)),
                created_at INTEGER NOT NULL);
            CREATE UNIQUE INDEX IF NOT EXISTS credential_active_namespace
                ON credential_accounts(provider_namespace) WHERE active=1;
            CREATE INDEX IF NOT EXISTS credential_namespace
                ON credential_accounts(provider_namespace,created_at);
            INSERT OR IGNORE INTO schema_migrations(version,applied_at) VALUES(11,'t53-credentials');")?;
        tx.commit()?;
        Ok(())
    }

    /// Store material and make it active atomically. Config/env are never copied here.
    pub fn add_credential(
        &self,
        namespace: &str,
        label: &str,
        material: CredentialMaterial,
    ) -> Result<AccountSummary, StorageError> {
        if !metadata(namespace, 4096) || !metadata(label, 128) || !material.valid() {
            return Err(StorageError::InvalidCredential);
        }
        let raw = serde_json::to_string(&material).map_err(|_| StorageError::InvalidCredential)?;
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(credential_error)?;
        let id: String = tx
            .query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))
            .map_err(credential_error)?;
        let created_at = tool_output::timestamp();
        tx.execute(
            "UPDATE credential_accounts SET active=0 WHERE provider_namespace=?1 AND active=1",
            [namespace],
        )
        .map_err(credential_error)?;
        tx.execute("INSERT INTO credential_accounts(id,provider_namespace,label,tagged_value_json,active,created_at) VALUES(?1,?2,?3,?4,1,?5)", params![id,namespace,label,raw,created_at]).map_err(credential_error)?;
        tx.commit().map_err(credential_error)?;
        Ok(AccountSummary {
            id,
            provider_namespace: namespace.into(),
            label: label.into(),
            kind: material.kind(),
            active: true,
            created_at,
        })
    }

    /// Metadata-only listing. JSON extraction avoids reading token material into DTOs.
    pub fn credential_accounts(
        &self,
        namespace: &str,
    ) -> Result<Vec<AccountSummary>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut statement = conn.prepare("SELECT id,provider_namespace,label,json_extract(tagged_value_json,'$.type'),active,created_at FROM credential_accounts WHERE provider_namespace=?1 ORDER BY created_at DESC,rowid DESC").map_err(credential_error)?;
        let rows = statement
            .query_map([namespace], |row| {
                let kind: String = row.get(3)?;
                let kind = match kind.as_str() {
                    "key" => CredentialKind::Key,
                    "oauth" => CredentialKind::OAuth,
                    _ => return Err(rusqlite::Error::InvalidQuery),
                };
                Ok(AccountSummary {
                    id: row.get(0)?,
                    provider_namespace: row.get(1)?,
                    label: row.get(2)?,
                    kind,
                    active: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(credential_error)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(credential_error)
    }

    /// Only the resolver reads material; no public UI/query path returns it.
    pub(crate) fn active_credential(
        &self,
        namespace: &str,
    ) -> Result<Option<CredentialMaterial>, StorageError> {
        let raw: Option<String> = self.conn.lock().expect("db mutex").query_row("SELECT tagged_value_json FROM credential_accounts WHERE provider_namespace=?1 AND active=1", [namespace], |r| r.get(0)).optional().map_err(credential_error)?;
        raw.map(|raw| {
            let material: CredentialMaterial =
                serde_json::from_str(&raw).map_err(|_| StorageError::InvalidCredential)?;
            if !material.valid() {
                return Err(StorageError::InvalidCredential);
            }
            Ok(material)
        })
        .transpose()
    }

    /// Activate one account in its exact namespace, with no transient double-active state.
    pub fn activate_credential(&self, namespace: &str, id: &str) -> Result<(), StorageError> {
        self.mutate_credential(namespace, id, None, false)
    }

    /// Rename account metadata, never its namespace or material.
    pub fn rename_credential(
        &self,
        namespace: &str,
        id: &str,
        label: &str,
    ) -> Result<(), StorageError> {
        if !metadata(label, 128) {
            return Err(StorageError::InvalidCredential);
        }
        self.mutate_credential(namespace, id, Some(label), false)
    }

    /// Remove an account; removing active selects the newest remaining account.
    pub fn remove_credential(&self, namespace: &str, id: &str) -> Result<(), StorageError> {
        self.mutate_credential(namespace, id, None, true)
    }

    fn mutate_credential(
        &self,
        namespace: &str,
        id: &str,
        label: Option<&str>,
        remove: bool,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(credential_error)?;
        let active: Option<bool> = tx
            .query_row(
                "SELECT active FROM credential_accounts WHERE provider_namespace=?1 AND id=?2",
                params![namespace, id],
                |r| r.get(0),
            )
            .optional()
            .map_err(credential_error)?;
        let active = active.ok_or(StorageError::CredentialNotFound)?;
        if let Some(label) = label {
            tx.execute(
                "UPDATE credential_accounts SET label=?3 WHERE provider_namespace=?1 AND id=?2",
                params![namespace, id, label],
            )
            .map_err(credential_error)?;
        } else if remove {
            tx.execute(
                "DELETE FROM credential_accounts WHERE provider_namespace=?1 AND id=?2",
                params![namespace, id],
            )
            .map_err(credential_error)?;
            if active {
                tx.execute("UPDATE credential_accounts SET active=1 WHERE id=(SELECT id FROM credential_accounts WHERE provider_namespace=?1 ORDER BY created_at DESC,rowid DESC LIMIT 1)", [namespace]).map_err(credential_error)?;
            }
        } else {
            tx.execute(
                "UPDATE credential_accounts SET active=0 WHERE provider_namespace=?1 AND active=1",
                [namespace],
            )
            .map_err(credential_error)?;
            tx.execute(
                "UPDATE credential_accounts SET active=1 WHERE provider_namespace=?1 AND id=?2",
                params![namespace, id],
            )
            .map_err(credential_error)?;
        }
        tx.commit().map_err(credential_error)?;
        Ok(())
    }
}
