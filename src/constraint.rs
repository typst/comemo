use std::cell::{Cell, RefCell};
use std::hash::Hash;

use siphasher::sip128::{Hasher128, SipHasher};

/// Extend an outer constraint by an inner one.
pub trait Join<T = Self> {
    /// Join this constraint with the `inner` one.
    fn join(&self, inner: &T);
}

impl<T: Join> Join<T> for Option<&T> {
    fn join(&self, inner: &T) {
        if let Some(outer) = self {
            outer.join(inner);
        }
    }
}

/// Defines a constraint for a tracked method without arguments.
#[derive(Default)]
pub struct SoloConstraint {
    cell: Cell<Option<u128>>,
}

impl SoloConstraint {
    /// Set the constraint for the value.
    pub fn set(&self, _: (), hash: u128) {
        self.cell.set(Some(hash));
    }

    /// Whether the value fulfills the constraint.
    pub fn valid<F>(&self, f: F) -> bool
    where
        F: Fn(()) -> u128,
    {
        self.cell.get().map_or(true, |hash| hash == f(()))
    }
}

impl Join for SoloConstraint {
    fn join(&self, inner: &Self) {
        let inner = inner.cell.get();
        if inner.is_some() {
            self.cell.set(inner);
        }
    }
}

/// Defines a constraint for a tracked method with arguments.
pub struct MultiConstraint<In> {
    calls: RefCell<Vec<(In, u128)>>,
}

impl<In> MultiConstraint<In>
where
    In: Clone + PartialEq,
{
    /// Enter a constraint for a pair of inputs and output.
    pub fn set(&self, input: In, hash: u128) {
        let mut calls = self.calls.borrow_mut();
        if calls.iter().all(|item| item.0 != input) {
            calls.push((input, hash));
        }
    }

    /// Whether the method satisfies as all input-output pairs.
    pub fn valid<F>(&self, f: F) -> bool
    where
        F: Fn(&In) -> u128,
    {
        let calls = self.calls.borrow();
        calls.iter().all(|(input, hash)| *hash == f(input))
    }
}

impl<In> Join for MultiConstraint<In>
where
    In: Clone + PartialEq,
{
    fn join(&self, inner: &Self) {
        let mut calls = self.calls.borrow_mut();
        let inner = inner.calls.borrow();
        for (input, hash) in inner.iter() {
            if calls.iter().all(|item| &item.0 != input) {
                calls.push((input.clone(), *hash));
            }
        }
    }
}

impl<In> Default for MultiConstraint<In> {
    fn default() -> Self {
        Self { calls: RefCell::new(vec![]) }
    }
}

/// Produce a 128-bit hash of a value.
pub fn hash<T: Hash>(value: &T) -> u128 {
    let mut state = SipHasher::new();
    value.hash(&mut state);
    state.finish128().as_u128()
}
