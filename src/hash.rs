use std::cell::Cell;
use std::hash::Hash;
use std::marker::PhantomData;

use siphasher::sip128::{Hasher128, SipHasher};

/// Defines a constraint for a value through its hash.
#[derive(Default)]
pub struct HashConstraint<T: Hash> {
    cell: Cell<Option<u128>>,
    marker: PhantomData<T>,
}

impl<T: Hash> HashConstraint<T> {
    /// Set the constraint for the value.
    pub fn set(&self, value: &T) {
        self.cell.set(Some(hash(value)));
    }

    /// Whether the value fulfills the constraint.
    pub fn valid(&self, value: &T) -> bool {
        self.cell.get().map_or(true, |v| v == hash(value))
    }
}

/// Produce a non zero 128-bit hash of a value.
fn hash<T: Hash>(value: &T) -> u128 {
    let mut state = SipHasher::new();
    value.hash(&mut state);
    state.finish128().as_u128()
}
