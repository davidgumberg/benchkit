use anyhow::Result;
use std::collections::HashMap;

/// Trait for types that can be merged with overriding values
pub trait Merge<T = Self> {
    /// Merge self with another instance, with the other instance taking precedence
    /// for any overlapping fields
    fn merge(&self, other: &T) -> Result<Self>
    where
        Self: Sized;
}

/// Trait for types that can be merged with a HashMap of overriding values
pub trait MergeFromMap<K, V> {
    /// Merge self with a map of values, with the map taking precedence
    fn merge_from_map(&self, map: &HashMap<K, V>) -> Result<Self>
    where
        Self: Sized;
}
