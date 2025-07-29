use std::collections::HashSet;
use std::hash::Hash;

use crate::Track;

/// A call to a tracked function.
pub trait Call: Hash + PartialEq + Clone + Send + Sync {
    /// Whether the call is mutable.
    fn is_mutable(&self) -> bool;
}

/// This implementation is used for hashed types in the `Input` trait.
impl Call for () {
    fn is_mutable(&self) -> bool {
        false
    }
}

/// Records and deduplicates calls.
#[derive(Clone)]
pub struct Constraint<T: Track> {
    /// The raw calls. In order, but deduplicated via the `map`.
    vec: Vec<(T::Call, u128)>,
    /// A map from hashes of calls to the indices in the vector.
    seen: HashSet<u128>,
}

impl<T: Track> Constraint<T> {
    /// Creates new empty constraints.
    pub fn new() -> Self {
        Constraint { vec: Vec::new(), seen: HashSet::new() }
    }

    /// Pushes a pair of a call and its return hash.
    pub fn push(&mut self, call: T::Call, ret: u128) {
        if self.seen.insert(crate::hash::hash(&call)) {
            self.vec.push((call, ret));
        }
    }

    /// Validates that the values matches the constraints.
    pub fn validate(self, value: &T) -> bool {
        self.vec.into_iter().all(|(call, ret)| value.call(call) == ret)
    }
}

impl<T: Track> Default for Constraint<T> {
    fn default() -> Self {
        Self::new()
    }
}
