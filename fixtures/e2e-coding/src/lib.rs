//! E2E01 seeded fixture: `add` subtracts (bug); `version` is the
//! protected public API that must survive the fix unchanged.

/// Add two integers (currently wrong: subtracts).
pub fn add(a: i32, b: i32) -> i32 {
    a - b
}

/// Protected public API: must remain exactly `"1.0.0"`.
pub fn version() -> &'static str {
    "1.0.0"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds() {
        assert_eq!(add(2, 3), 5);
    }

    #[test]
    fn api_stable() {
        assert_eq!(version(), "1.0.0");
    }
}
