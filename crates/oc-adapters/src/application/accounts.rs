//! Safe account projection/mutation through the existing SQLite and request owners.
use super::*;
use oc_core::queries::{
    AccountAction, AccountAuthSource, AccountKind, ProviderAccount, ProviderAccounts,
};

fn scope(c: &Composition, provider: &str) -> Result<crate::auth::AuthScope, CoreError> {
    let base = if provider == crate::models_dev::PROVIDER && c.generation.public_go_enabled {
        crate::auth::GO_BASE_URL
    } else {
        let entry = c
            .generation
            .providers
            .get(provider)
            .ok_or_else(|| CoreError::Application("provider connection unavailable".into()))?;
        crate::endpoint::EndpointBinding::admit(
            &entry.options.base_url,
            entry.options.endpoint_trusted,
            entry.options.endpoint_source.as_deref().unwrap_or_default(),
        )
        .map_err(|_| CoreError::Application("provider connection unavailable".into()))?;
        &entry.options.base_url
    };
    crate::auth::AuthScope::admit(provider, base)
        .map_err(|_| CoreError::Application("provider connection unavailable".into()))
}

pub(super) fn apply(
    db: &Db,
    runtime: &Runtime<'_>,
    c: &mut Composition,
    provider: String,
    action: Option<AccountAction>,
) -> Result<ProviderAccounts, CoreError> {
    let scope = scope(c, &provider)?;
    if let Some(action) = action {
        // Account management never mutates an already prepared request or active child.
        if runtime.turn_active() {
            return Err(CoreError::TurnBusy);
        }
        let result = match action {
            AccountAction::AddKey { label, key } => db
                .add_credential(
                    scope.namespace(),
                    &label,
                    crate::storage::CredentialMaterial::Key {
                        key: key.into_secret(),
                    },
                )
                .map(|_| ()),
            AccountAction::Activate { id } => db.activate_credential(scope.namespace(), &id),
            AccountAction::Rename { id, label } => {
                db.rename_credential(scope.namespace(), &id, &label)
            }
            AccountAction::Remove {
                id,
                confirmed: true,
            } => db.remove_credential(scope.namespace(), &id),
            AccountAction::Remove {
                confirmed: false, ..
            } => {
                return Err(CoreError::Application(
                    "account removal requires confirmation".into(),
                ));
            }
        };
        result.map_err(|error| query_storage_error(db, error))?;
        if c.catalog.provider == provider {
            c.refresh_credentials(db)
                .map_err(|composition::LoadFailure::Configuration(d)| CoreError::Diagnostic(d))?;
            runtime
                .publish_provider_credentials(&c.generation)
                .map_err(runtime_error)?;
            runtime
                .publish_provider_state(c.provider_state.clone())
                .map_err(runtime_error)?;
        }
    }
    read(db, c, provider)
}

pub(super) fn read(
    db: &Db,
    c: &Composition,
    provider: String,
) -> Result<ProviderAccounts, CoreError> {
    let scope = scope(c, &provider)?;
    let options = c.generation.providers.get(&provider).map(|e| &e.options);
    let resolved = scope
        .resolve(
            db,
            options.map(|o| o.auth_policy).unwrap_or_default(),
            options.map(|o| o.api_key.as_str()),
            options.is_some_and(|o| {
                o.headers.keys().any(|n| {
                    n.eq_ignore_ascii_case("authorization") || n.eq_ignore_ascii_case("x-api-key")
                })
            }),
            || c.parent_env.get("OPENCODE_API_KEY").cloned(),
        )
        .map_err(|_| CoreError::Application("account resolution unavailable".into()))?;
    let effective = match resolved.source {
        crate::auth::AuthSource::Stored => AccountAuthSource::Stored,
        crate::auth::AuthSource::GoEnvironment => AccountAuthSource::Environment,
        crate::auth::AuthSource::Configured => AccountAuthSource::Configured,
        crate::auth::AuthSource::Anonymous => AccountAuthSource::Anonymous,
        crate::auth::AuthSource::UnsupportedOAuth => AccountAuthSource::UnsupportedOAuth,
        crate::auth::AuthSource::Missing => AccountAuthSource::Missing,
    };
    Ok(ProviderAccounts {
        provider,
        effective,
        accounts: db
            .credential_accounts(scope.namespace())
            .map_err(|error| query_storage_error(db, error))?
            .into_iter()
            .map(|a| ProviderAccount {
                id: a.id,
                label: a.label,
                kind: match a.kind {
                    crate::storage::CredentialKind::Key => AccountKind::Key,
                    crate::storage::CredentialKind::OAuth => AccountKind::OAuth,
                },
                active: a.active,
                created_at: a.created_at,
            })
            .collect(),
    })
}
