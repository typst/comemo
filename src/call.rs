use std::collections::HashSet;
use std::hash::Hash;

use parking_lot::Mutex;

use crate::Track;
use crate::track::Sink;

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
pub struct Constraint<T: Track>(Mutex<Inner<T>>);

struct Inner<T: Track> {
    /// The raw calls. In order, but deduplicated via the `map`.
    vec: Vec<(T::Call, u128)>,
    /// A map from hashes of calls to the indices in the vector.
    seen: HashSet<u128>,
}

impl<T: Track> Constraint<T> {
    /// Creates new empty constraints.
    pub fn new() -> Self {
        Constraint(Mutex::new(Inner { vec: Vec::new(), seen: HashSet::new() }))
    }

    /// Validates that the values matches the constraints.
    pub fn validate(self, value: &T) -> bool {
        self.0
            .into_inner()
            .vec
            .into_iter()
            .all(|(call, ret)| value.call(call) == ret)
    }
}

impl<T: Track> Default for Constraint<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Track> Sink for Constraint<T> {
    type Call = T::Call;

    fn emit(&self, call: Self::Call, ret: u128) -> bool {
        let mut inner = self.0.lock();
        if inner.seen.insert(crate::hash::hash(&call)) {
            inner.vec.push((call, ret));
            true
        } else {
            false
        }
    }
}
