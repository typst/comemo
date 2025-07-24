use std::fmt::Debug;
use std::hash::Hash;

use siphasher::sip128::{Hasher128, SipHasher13};

/// A call to a tracked function.
pub trait Call: Debug + Hash + PartialEq + Clone + Send + Sync {
    /// Whether the call is mutable.
    fn is_mutable(&self) -> bool;
}

impl Call for () {
    fn is_mutable(&self) -> bool {
        false
    }
}

/// Produce a 128-bit hash of a value.
#[inline]
pub fn hash<T: Hash>(value: &T) -> u128 {
    let mut state = SipHasher13::new();
    value.hash(&mut state);
    state.finish128().as_u128()
}
