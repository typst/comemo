use std::cell::{Cell, RefCell};
use std::hash::Hash;

use siphasher::sip128::{Hasher128, SipHasher};

/// Ensure a type is suitable as an argument to a tracked function.
pub fn assert_clone_and_partial_eq<T: Clone + PartialEq>() {}

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

/// Defines a constraint for a value through its hash.
#[derive(Default)]
pub struct HashConstraint {
    cell: Cell<Option<u128>>,
}

impl HashConstraint {
    /// Set the constraint for the value.
    pub fn set<T: Hash>(&self, value: &T) {
        self.cell.set(Some(siphash(value)));
    }

    /// Whether the value fulfills the constraint.
    pub fn valid<T: Hash>(&self, value: &T) -> bool {
        self.cell.get().map_or(true, |hash| hash == siphash(value))
    }
}

impl Join for HashConstraint {
    fn join(&self, inner: &Self) {
        let inner = inner.cell.get();
        if inner.is_some() {
            self.cell.set(inner);
        }
    }
}

/// Defines a constraint for a function by keeping track of all invocations.
pub struct FuncConstraint<In> {
    calls: RefCell<Vec<(In, u128)>>,
}

impl<In> FuncConstraint<In>
where
    In: Clone + PartialEq,
{
    /// Enter a constraint for a pair of inputs and output.
    pub fn set<Out: Hash>(&self, input: In, output: &Out) {
        let mut calls = self.calls.borrow_mut();
        if calls.iter().all(|item| item.0 != input) {
            calls.push((input, siphash(output)));
        }
    }

    /// Whether the function satifies as all input-output pairs.
    pub fn valid<Out, F>(&self, f: F) -> bool
    where
        Out: Hash,
        F: Fn(&In) -> Out,
    {
        let calls = self.calls.borrow();
        calls.iter().all(|(input, hash)| *hash == siphash(&f(input)))
    }
}

impl<In> Default for FuncConstraint<In> {
    fn default() -> Self {
        Self { calls: RefCell::new(vec![]) }
    }
}

impl<In> Join for FuncConstraint<In>
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

/// Produce a 128-bit hash of a value.
fn siphash<T: Hash>(value: &T) -> u128 {
    let mut state = SipHasher::new();
    value.hash(&mut state);
    state.finish128().as_u128()
}
