//! Immutable-by-default shared data with copy-on-write mutation.
//!
//! Admitted wire bindings and catalogs are captured into every prepared
//! request and tool batch. They are read far more often than changed, so a
//! clone shares one allocation and a mutation copies only a shared value.
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Shared<T>(Arc<T>);

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> std::ops::Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Clone> std::ops::DerefMut for Shared<T> {
    fn deref_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.0)
    }
}

impl<T> From<T> for Shared<T> {
    fn from(value: T) -> Self {
        Self(Arc::new(value))
    }
}

impl<K: Ord, V, const N: usize> From<[(K, V); N]> for Shared<BTreeMap<K, V>> {
    fn from(entries: [(K, V); N]) -> Self {
        Self::from(BTreeMap::from(entries))
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for Shared<BTreeMap<K, V>> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self::from(iter.into_iter().collect::<BTreeMap<_, _>>())
    }
}

impl<'a, K, V> IntoIterator for &'a Shared<BTreeMap<K, V>> {
    type Item = (&'a K, &'a V);
    type IntoIter = std::collections::btree_map::Iter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<T: Clone> Shared<T> {
    /// Owned value without copying when this is the only reference.
    pub fn into_inner(self) -> T {
        Arc::unwrap_or_clone(self.0)
    }
}

impl<T> Shared<T> {
    #[cfg(test)]
    pub(crate) fn shares(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T: serde::Serialize> serde::Serialize for Shared<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Shared<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::from)
    }
}
