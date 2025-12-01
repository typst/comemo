use std::hash::Hash;

use siphasher::sip128::{Hasher128, SipHasher13};

/// Produce a 128-bit hash of a value.
#[inline]
pub fn hash<T: Hash>(value: &T) -> u128 {
    let mut state = SipHasher13::new();
    value.hash(&mut state);
    state.finish128().as_u128()
}
