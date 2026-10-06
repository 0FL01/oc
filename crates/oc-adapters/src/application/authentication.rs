//! One application-lifetime OpenAI attempt owner, independent of Location/runtime.
use super::*;
use oc_core::queries::{AuthAction, AuthAttempt, AuthMethod, OAuthMethod};

pub(super) type Owner = Result<crate::auth::OpenAiAttempts, CoreError>;

pub(super) fn new(db: &Db) -> Owner {
    crate::auth::OpenAiAttempts::new(db).map_err(error)
}

fn error(error: crate::auth::AuthError) -> CoreError {
    // AuthError has fixed, credential/URL/body-free messages.
    CoreError::Application(error.to_string())
}

pub(super) fn methods(c: &Composition, provider: &str) -> Result<Vec<AuthMethod>, CoreError> {
    let scope = accounts::scope(c, provider)?;
    let builtin =
        crate::auth::AuthScope::admit("openai", crate::auth::OPENAI_BASE_URL).map_err(error)?;
    if scope.namespace() == builtin.namespace() {
        Ok(vec![
            AuthMethod::OAuth(OAuthMethod::Browser),
            AuthMethod::OAuth(OAuthMethod::Device),
            AuthMethod::Key,
        ])
    } else {
        Ok(vec![AuthMethod::Key])
    }
}

pub(super) async fn action(
    owner: &Owner,
    c: &Composition,
    provider: &str,
    action: AuthAction,
) -> Result<AuthAttempt, CoreError> {
    if provider != "openai" {
        return Err(CoreError::Application("OAuth provider unavailable".into()));
    }
    let owner = owner.as_ref().map_err(Clone::clone)?;
    match action {
        AuthAction::Begin { method, label } => {
            if !methods(c, provider)?.contains(&AuthMethod::OAuth(method)) {
                return Err(CoreError::Application("OAuth provider unavailable".into()));
            }
            owner.begin(method, label).await.map_err(error)
        }
        // An owned attempt remains cancellable/readable after a Location/config
        // switch. It never becomes a login to that Location's custom endpoint.
        AuthAction::Status { attempt } => owner.status(&attempt).map_err(error),
        AuthAction::Cancel { attempt } => owner.cancel(&attempt).await.map_err(error),
    }
}

pub(super) async fn shutdown(owner: &Owner) -> Result<(), CoreError> {
    owner
        .as_ref()
        .map_err(Clone::clone)?
        .shutdown()
        .await
        .map_err(error)
}
