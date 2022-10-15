use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::hash::Hash;

use siphasher::sip128::{Hasher128, SipHasher};

/// Extend an outer constraint by an inner one.
pub trait Join<T = Self> {
    /// Join this constraint with the `inner` one.
    fn join(&self, inner: &T);
}

impl<T: Join> Join<T> for Option<&T> {
    #[inline]
    fn join(&self, inner: &T) {
        if let Some(outer) = self {
            outer.join(inner);
        }
    }
}

/// Defines a constraint for a tracked method without arguments.
#[derive(Debug, Default)]
pub struct SoloConstraint {
    cell: Cell<Option<u128>>,
}

impl SoloConstraint {
    /// Set the constraint for the value.
    #[inline]
    #[track_caller]
    pub fn set(&self, _: (), hash: u128) {
        // If there's already a constraint, it must match. This assertion can
        // fail if a tracked function isn't pure (which violates comemo's
        // contract).
        if let Some(existing) = self.cell.get() {
            check(hash, existing);
        } else {
            self.cell.set(Some(hash));
        }
    }

    /// Whether the value fulfills the constraint.
    #[inline]
    pub fn valid<F>(&self, f: F) -> bool
    where
        F: Fn(()) -> u128,
    {
        self.cell.get().map_or(true, |hash| hash == f(()))
    }
}

impl Join for SoloConstraint {
    #[inline]
    fn join(&self, inner: &Self) {
        if let Some(hash) = inner.cell.get() {
            self.set((), hash);
        }
    }
}

/// Defines a constraint for a tracked method with arguments.
#[derive(Debug)]
pub struct MultiConstraint<In> {
    calls: RefCell<Vec<(In, u128)>>,
}

impl<In> MultiConstraint<In>
where
    In: Clone + PartialEq,
{
    /// Enter a constraint for a pair of inputs and output.
    #[inline]
    #[track_caller]
    pub fn set(&self, input: In, hash: u128) {
        let mut calls = self.calls.borrow_mut();
        if let Some(item) = calls.iter().find(|item| item.0 == input) {
            check(item.1, hash);
        } else {
            calls.push((input, hash));
        }
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn valid<F>(&self, f: F) -> bool
    where
        F: Fn(&In) -> u128,
    {
        self.calls.borrow().iter().all(|(input, hash)| *hash == f(input))
    }
}

impl<In> Join for MultiConstraint<In>
where
    In: Debug + Clone + PartialEq,
{
    #[inline]
    fn join(&self, inner: &Self) {
        let mut calls = self.calls.borrow_mut();
        for (input, hash) in inner.calls.borrow().iter() {
            if let Some(item) = calls.iter().find(|item| &item.0 == input) {
                check(item.1, *hash);
            } else {
                calls.push((input.clone(), *hash));
            }
        }
    }
}

impl<In> Default for MultiConstraint<In> {
    #[inline]
    fn default() -> Self {
        Self { calls: RefCell::new(vec![]) }
    }
}

/// Check for a constraint violation.
#[inline]
#[track_caller]
fn check(left: u128, right: u128) {
    if left != right {
        panic!(
            "comemo: found conflicting constraints. \
             is this tracked function pure?"
        )
    }
}

/// Produce a 128-bit hash of a value.
#[inline]
pub fn hash<T: Hash>(value: &T) -> u128 {
    let mut state = SipHasher::new();
    value.hash(&mut state);
    state.finish128().as_u128()
}
