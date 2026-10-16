//! Minimal runtime marker for T01.
//! Session worker / turn / supervisor arrive in M1 (T03).

/// Runtime smoke marker: proves Tokio is linked into `oc-core`.
pub async fn smoke_tick() -> u64 {
    tokio::task::yield_now().await;
    1
}

#[cfg(test)]
mod tests {
    use super::smoke_tick;

    #[tokio::test]
    async fn tick_returns_one() {
        assert_eq!(smoke_tick().await, 1);
    }
}
