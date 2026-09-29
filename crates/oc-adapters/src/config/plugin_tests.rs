//! CFG08/CFG09: rejected identities are opaque even in public typed errors.
use super::*;

#[test]
fn cfg09_rejected_plugin_errors_never_retain_raw_identity() {
    for identity in [
        "CFG09_PLAIN_SECRET_39a2",
        "https://user:CFG09_AUTH_SECRET_39a2@remote.invalid/private/plugin.js?key=secret\x1b[31m",
        "/private/CFG09_PATH_SECRET_39a2/openproxy-models.js",
        "@tarquinen/opencode-dcp@3.1.16",
    ] {
        let error = classify_plugin(identity, "/admitted").unwrap_err();
        let resolved = crate::dcp_auto::resolve_dcp_module(identity).unwrap_err();
        let rendered = format!("{error:?} {error} {resolved:?} {resolved}");
        assert!(!rendered.contains(identity));
        assert!(
            !rendered.contains("SECRET")
                && !rendered.contains('\x1b')
                && !rendered.contains("https://")
        );
        let ConfigError::UnsupportedPlugin {
            identity: opaque, ..
        } = error
        else {
            panic!("typed rejection")
        };
        assert_eq!(opaque, safe_plugin_id(identity));
        assert!(opaque.starts_with("plugin-"));
        assert_eq!(opaque.len(), 71);
    }
    assert_ne!(safe_plugin_id("one"), safe_plugin_id("two"));
}
