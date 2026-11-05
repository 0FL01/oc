//! Captured, trusted formatter argv; never model tool input or a Shell grant.
use super::*;

/// One configured formatter in the immutable execution generation.
#[derive(Clone, Default, PartialEq)]
pub struct Formatter {
    pub(crate) command: Vec<String>,
    pub(crate) environment: BTreeMap<String, String>,
    pub(crate) extensions: Vec<String>,
    pub(crate) disabled: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Declarations {
    Reset(bool),
    Named(OrderedEntries),
}

struct OrderedEntries(Vec<(String, serde_json::Value)>);
impl<'de> Deserialize<'de> for OrderedEntries {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = OrderedEntries;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("configured formatters")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(OrderedEntries(entries))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

impl std::fmt::Debug for Formatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Formatter")
            .field("arguments", &self.command.len())
            .field("environment_entries", &self.environment.len())
            .field("extensions", &self.extensions.len())
            .field("disabled", &self.disabled)
            .finish()
    }
}

pub(super) fn merge(
    definitions: &mut Vec<(String, Formatter)>,
    source: &Source,
    env: &BTreeMap<String, String>,
    reader: &impl Fn(&str, &str) -> Result<String, ConfigError>,
) -> Result<(), ConfigError> {
    let invalid = || ConfigError::Invalid {
        field: "formatter".into(),
        reason: "expected boolean or formatter objects with argv, environment and extensions"
            .into(),
    };
    // serde_json::Value sorts map keys. Formatter order is observable (the
    // first successful program wins), so decode this one map in source order.
    #[derive(Deserialize)]
    struct Document {
        formatter: Declarations,
    }
    let document: Document =
        serde_json::from_str(&strip_jsonc(&source.text)?).map_err(|_| invalid())?;
    let entries = match document.formatter {
        Declarations::Reset(enabled) => {
            let _ = enabled;
            definitions.clear();
            return Ok(());
        }
        Declarations::Named(entries) => entries.0,
    };
    if !source.trusted {
        return Err(ConfigError::Untrusted {
            origin: mcp::safe_source_id(&source.path),
            reason: "formatter execution requires an admitted source".into(),
        });
    }
    for (name, value) in entries {
        let object = value.as_object().ok_or_else(invalid)?;
        let index = definitions
            .iter()
            .position(|(id, _)| *id == name)
            .unwrap_or_else(|| {
                definitions.push((name, Formatter::default()));
                definitions.len() - 1
            });
        let entry = &mut definitions[index].1;
        if let Some(value) = object.get("disabled") {
            entry.disabled = value.as_bool().ok_or_else(invalid)?;
        }
        let text = |value: &serde_json::Value| {
            let value = value
                .as_str()
                .filter(|s| !s.contains('\0'))
                .ok_or_else(invalid)?;
            let value = substitute_with(value, &source.path, source.trusted, env, reader)?;
            if value.contains('\0') {
                return Err(invalid());
            }
            Ok(value)
        };
        if let Some(value) = object.get("command") {
            entry.command = value
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .map(text)
                .collect::<Result<_, _>>()?;
            if entry.command.first().is_none_or(String::is_empty) {
                return Err(invalid());
            }
        }
        if let Some(value) = object.get("extensions") {
            entry.extensions = value
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .map(text)
                .collect::<Result<_, _>>()?;
        }
        if let Some(value) = object.get("environment") {
            for (key, value) in value.as_object().ok_or_else(invalid)? {
                if key.is_empty() || key.contains(['\0', '=']) {
                    return Err(invalid());
                }
                entry.environment.insert(key.clone(), text(value)?);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w10_formatter_capture_preserves_program_order_reset_and_source_trust() {
        let mut definitions = Vec::new();
        let mut source = Source {
            path: "formatter-fixture".into(), trusted: true,
            text: r#"{"formatter":{"z-first":{"command":["first","{env:CAPTURED}"],"extensions":[".txt"]},"a-second":{"command":["second"],"extensions":[".txt"]}}}"#.into(),
        };
        let env = BTreeMap::from([("CAPTURED".into(), "FORMATTER_VALUE_CANARY".into())]);
        let reader = |_: &str, _: &str| -> Result<String, ConfigError> {
            panic!("formatter fixture must not read files")
        };
        merge(&mut definitions, &source, &env, &reader).unwrap();
        assert_eq!(
            definitions
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["z-first", "a-second"]
        );
        assert_eq!(definitions[0].1.command[1], "FORMATTER_VALUE_CANARY");
        assert!(!format!("{:?}", definitions[0].1).contains("FORMATTER_VALUE_CANARY"));
        source.text = r#"{"formatter":{"z-first":{"disabled":true}}}"#.into();
        merge(&mut definitions, &source, &env, &reader).unwrap();
        assert!(definitions[0].1.disabled);
        assert_eq!(definitions[0].1.command[0], "first");
        source.text = r#"{"formatter":false}"#.into();
        merge(&mut definitions, &source, &env, &reader).unwrap();
        assert!(definitions.is_empty());
        source.text = r#"{"formatter":{"unsafe":{"command":["{file:secret}"]}}}"#.into();
        source.trusted = false;
        assert!(matches!(
            merge(&mut definitions, &source, &env, &reader),
            Err(ConfigError::Untrusted { .. })
        ));
        source.trusted = true;
        for text in [
            r#"{"formatter":{"bad":{"command":[]}}}"#,
            r#"{"formatter":{"bad":{"extensions":false}}}"#,
        ] {
            source.text = text.into();
            assert!(matches!(
                merge(&mut definitions, &source, &env, &reader),
                Err(ConfigError::Invalid { .. })
            ));
        }
    }
}
