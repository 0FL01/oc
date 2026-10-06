//! Non-default binary-fixture authority. No production/default-build override.
use super::AuthError;

pub(super) fn origin() -> Result<String, AuthError> {
    let raw = std::env::var("OC_AUTH_FIXTURE_ORIGIN").map_err(|_| AuthError::Authority)?;
    if raw.len() > 128 || raw.chars().any(char::is_control) {
        return Err(AuthError::Authority);
    }
    let url = reqwest::Url::parse(&raw).map_err(|_| AuthError::Authority)?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none_or(|port| port == 0)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AuthError::Authority);
    }
    Ok(url.as_str().trim_end_matches('/').into())
}

pub(super) fn route(
    subscription: bool,
) -> Result<(String, crate::endpoint::EndpointBinding), AuthError> {
    let url = format!(
        "{}/{}",
        origin()?,
        if subscription { "codex" } else { "key" }
    );
    let endpoint = crate::endpoint::EndpointBinding::admit(&url, true, "AUTH binary fixture")
        .map_err(|_| AuthError::Authority)?;
    Ok((url, endpoint))
}
