//! Narrow port traits for T01.
//! Full Provider/Store/ToolExecutor/ContextPolicy arrive in M1–M4.

use thiserror::Error;

/// Category-only error for the T01 smoke port.
#[derive(Debug, Error)]
pub enum PortError {
    /// Smoke transport is unavailable; no retry or side effect.
    #[error("smoke unavailable")]
    Unavailable,
}

/// Minimal provider port: request text in, streamed text out (smoke only).
pub trait ProviderPort: Send + Sync {
    /// Return a single smoke chunk for the given prompt.
    #[allow(clippy::manual_async_fn)]
    fn smoke_complete(
        &self,
        prompt: &str,
    ) -> impl Future<Output = Result<String, PortError>> + Send;
}

/// Minimal store port: durable smoke write/read.
pub trait StorePort: Send + Sync {
    /// Persist a smoke record and return its row id.
    fn smoke_write(&self, value: &str) -> Result<i64, PortError>;
}

#[cfg(test)]
mod tests {
    use super::{PortError, ProviderPort};

    struct Never;

    impl ProviderPort for Never {
        #[allow(clippy::manual_async_fn)]
        fn smoke_complete(
            &self,
            _prompt: &str,
        ) -> impl Future<Output = Result<String, PortError>> + Send {
            async { Err(PortError::Unavailable) }
        }
    }

    #[tokio::test]
    async fn smoke_provider_error_is_typed() {
        let err = Never.smoke_complete("hi").await.expect_err("must fail");
        assert_eq!(err.to_string(), "smoke unavailable");
    }
}
