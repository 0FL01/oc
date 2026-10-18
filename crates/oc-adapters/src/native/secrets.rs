//! Private, non-Debug wire values. Public KeyInput remains nonserializable.
use oc_core::queries::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
enum Account {
    AddKey { label: String, key: String },
    Activate { id: String },
    Rename { id: String, label: String },
    Remove { id: String, confirmed: bool },
}
impl From<AccountAction> for Account {
    fn from(action: AccountAction) -> Self {
        match action {
            AccountAction::AddKey { label, key } => Self::AddKey {
                label,
                key: key.into_secret(),
            },
            AccountAction::Activate { id } => Self::Activate { id },
            AccountAction::Rename { id, label } => Self::Rename { id, label },
            AccountAction::Remove { id, confirmed } => Self::Remove { id, confirmed },
        }
    }
}
impl From<Account> for AccountAction {
    fn from(action: Account) -> Self {
        match action {
            Account::AddKey { label, key } => Self::AddKey {
                label,
                key: KeyInput::new(key),
            },
            Account::Activate { id } => Self::Activate { id },
            Account::Rename { id, label } => Self::Rename { id, label },
            Account::Remove { id, confirmed } => Self::Remove { id, confirmed },
        }
    }
}
pub(super) mod account {
    use super::*;
    pub(crate) fn serialize<S: serde::Serializer>(
        value: &Option<AccountAction>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.clone().map(Account::from).serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<AccountAction>, D::Error> {
        Ok(Option::<Account>::deserialize(deserializer)?.map(AccountAction::from))
    }
}

#[derive(Serialize, Deserialize)]
enum Custom {
    Probe {
        name: String,
        base_url: String,
        key: Option<String>,
    },
    Retry {
        revision: u64,
    },
    Save {
        revision: u64,
        protocol: NativeProtocol,
    },
    Cancel {
        revision: u64,
    },
    Refresh {
        provider: String,
    },
}
pub(super) mod custom_provider {
    use super::*;
    pub(crate) fn serialize<S: serde::Serializer>(
        value: &CustomProviderAction,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let value = match value.clone() {
            CustomProviderAction::Probe { input, .. } => Custom::Probe {
                name: input.name,
                base_url: input.base_url,
                key: match input.auth {
                    CustomProviderAuth::Key(key) => Some(key.into_secret()),
                    CustomProviderAuth::Anonymous => None,
                },
            },
            CustomProviderAction::Retry { revision } => Custom::Retry { revision },
            CustomProviderAction::Save { revision, protocol } => {
                Custom::Save { revision, protocol }
            }
            CustomProviderAction::Cancel { revision } => Custom::Cancel { revision },
            CustomProviderAction::Refresh { provider } => Custom::Refresh { provider },
        };
        value.serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<CustomProviderAction, D::Error> {
        Ok(match Custom::deserialize(deserializer)? {
            Custom::Probe {
                name,
                base_url,
                key,
            } => CustomProviderAction::Probe {
                input: CustomProviderInput {
                    name,
                    base_url,
                    auth: key
                        .map(|key| CustomProviderAuth::Key(KeyInput::new(key)))
                        .unwrap_or(CustomProviderAuth::Anonymous),
                },
                cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
            Custom::Retry { revision } => CustomProviderAction::Retry { revision },
            Custom::Save { revision, protocol } => {
                CustomProviderAction::Save { revision, protocol }
            }
            Custom::Cancel { revision } => CustomProviderAction::Cancel { revision },
            Custom::Refresh { provider } => CustomProviderAction::Refresh { provider },
        })
    }
}
