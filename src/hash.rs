use std::hash::Hash;

use siphasher::sip128::{Hasher128, SipHasher};
use std::num::NonZeroU128;

/// Produce a non zero 128-bit hash of a value.
pub fn siphash<T: Hash>(value: &T) -> NonZeroU128 {
    let mut state = SipHasher::new();
    value.hash(&mut state);
    state
        .finish128()
        .as_u128()
        .try_into()
        .unwrap_or(NonZeroU128::new(u128::MAX).unwrap())
}
