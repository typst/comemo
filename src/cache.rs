use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::Hash;

use siphasher::sip128::{Hasher128, SipHasher13};

use crate::input::Input;

thread_local! {
    /// The global, dynamic cache shared by all memoized functions.
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());

    /// The global ID counter for tracked values. Each tracked value gets a
    /// unqiue ID based on which its validations are cached in the accelerator.
    /// IDs may only be reused upon eviction of the accelerator.
    static ID: Cell<usize> = const { Cell::new(0) };

    /// The global, dynamic accelerator shared by all cached values.
    static ACCELERATOR: RefCell<HashMap<(usize, u128), u128>>
        = RefCell::new(HashMap::default());
}

/// Execute a function or use a cached result for it.
pub fn memoized<'c, In, Out, F>(
    id: TypeId,
    mut input: In,
    constraint: &'c In::Constraint,
    func: F,
) -> Out
where
    In: Input + 'c,
    Out: Clone + 'static,
    F: FnOnce(In::Tracked<'c>) -> Out,
{
    CACHE.with(|cache| {
        // Compute the hash of the input's key part.
        let key = {
            let mut state = SipHasher13::new();
            input.key(&mut state);
            let hash = state.finish128().as_u128();
            (id, hash)
        };

        // Check if there is a cached output.
        let mut borrow = cache.borrow_mut();
        if let Some(constrained) = borrow.lookup::<In, Out>(key, &input) {
            // Replay the mutations.
            input.replay(&constrained.constraint);

            // Add the cached constraints to the outer ones.
            input.retrack(constraint).1.join(&constrained.constraint);

            let value = constrained.output.clone();
            borrow.last_was_hit = true;
            return value;
        }

        // Release the borrow so that nested memoized calls can access the
        // cache without panicking.
        drop(borrow);

        // Execute the function with the new constraints hooked in.
        let (input, outer) = input.retrack(constraint);
        let output = func(input);

        // Add the new constraints to the outer ones.
        outer.join(constraint);

        // Insert the result into the cache.
        borrow = cache.borrow_mut();
        borrow.insert::<In, Out>(key, constraint.take(), output.clone());
        borrow.last_was_hit = false;

        output
    })
}

/// Whether the last call was a hit.
pub fn last_was_hit() -> bool {
    CACHE.with(|cache| cache.borrow().last_was_hit)
}

/// Get the next ID.
pub fn id() -> usize {
    ID.with(|cell| {
        let current = cell.get();
        cell.set(current.wrapping_add(1));
        current
    })
}

/// Evict the cache.
///
/// This removes all memoized results from the cache whose age is larger than or
/// equal to `max_age`. The age of a result grows by one during each eviction
/// and is reset to zero when the result produces a cache hit. Set `max_age` to
/// zero to completely clear the cache.
///
/// Comemo's cache is thread-local, meaning that this only evicts this thread's
/// cache.
pub fn evict(max_age: usize) {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.map.retain(|_, entries| {
            entries.retain_mut(|entry| {
                entry.age += 1;
                entry.age <= max_age
            });
            !entries.is_empty()
        });
    });
    ACCELERATOR.with(|accelerator| accelerator.borrow_mut().clear());
}

/// The global cache.
#[derive(Default)]
struct Cache {
    /// Maps from function IDs + hashes to memoized results.
    map: HashMap<(TypeId, u128), Vec<CacheEntry>>,
    /// Whether the last call was a hit.
    last_was_hit: bool,
}

impl Cache {
    /// Look for a matching entry in the cache.
    fn lookup<In, Out>(
        &mut self,
        key: (TypeId, u128),
        input: &In,
    ) -> Option<&Constrained<In::Constraint, Out>>
    where
        In: Input,
        Out: Clone + 'static,
    {
        self.map
            .get_mut(&key)?
            .iter_mut()
            .rev()
            .find_map(|entry| entry.lookup::<In, Out>(input))
    }

    /// Insert an entry into the cache.
    fn insert<In, Out>(
        &mut self,
        key: (TypeId, u128),
        constraint: In::Constraint,
        output: Out,
    ) where
        In: Input,
        Out: 'static,
    {
        self.map
            .entry(key)
            .or_default()
            .push(CacheEntry::new::<In, Out>(constraint, output));
    }
}

/// A memoized result.
struct CacheEntry {
    /// The memoized function's constrained output.
    ///
    /// This is of type `Constrained<In::Constraint, Out>`.
    constrained: Box<dyn Any>,
    /// How many evictions have passed since the entry has been last used.
    age: usize,
}

/// A value with a constraint.
struct Constrained<C, T> {
    /// The constraint which must be fulfilled for the output to be used.
    constraint: C,
    /// The memoized function's output.
    output: T,
}

impl CacheEntry {
    /// Create a new entry.
    fn new<In, Out>(constraint: In::Constraint, output: Out) -> Self
    where
        In: Input,
        Out: 'static,
    {
        Self {
            constrained: Box::new(Constrained { constraint, output }),
            age: 0,
        }
    }

    /// Return the entry's output if it is valid for the given input.
    fn lookup<In, Out>(&mut self, input: &In) -> Option<&Constrained<In::Constraint, Out>>
    where
        In: Input,
        Out: Clone + 'static,
    {
        let constrained: &Constrained<In::Constraint, Out> =
            self.constrained.downcast_ref().expect("wrong entry type");

        input.validate(&constrained.constraint).then(|| {
            self.age = 0;
            constrained
        })
    }
}

/// Defines a constraint for a tracked type.
#[derive(Clone)]
pub struct Constraint<T>(RefCell<Vec<Call<T>>>);

/// A call entry.
#[derive(Clone)]
struct Call<T> {
    args: T,
    ret: u128,
    both: u128,
    mutable: bool,
}

impl<T: Hash + PartialEq> Constraint<T> {
    /// Create empty constraints.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a constraint for a call to an immutable function.
    #[inline]
    pub fn push(&self, args: T, ret: u128, mutable: bool) {
        let both = hash(&(&args, ret));
        self.push_inner(Call { args, ret, both, mutable });
    }

    /// Enter a constraint for a call to an immutable function.
    #[inline]
    fn push_inner(&self, call: Call<T>) {
        let mut calls = self.0.borrow_mut();

        if !call.mutable {
            for prev in calls.iter().rev() {
                if prev.mutable {
                    break;
                }

                #[cfg(debug_assertions)]
                if prev.args == call.args {
                    check(prev.ret, call.ret);
                }

                if prev.both == call.both {
                    return;
                }
            }
        }

        calls.push(call);
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn validate<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        self.0.borrow().iter().all(|entry| f(&entry.args) == entry.ret)
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn validate_with_id<F>(&self, mut f: F, id: usize) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        let calls = self.0.borrow();
        ACCELERATOR.with(|accelerator| {
            let mut map = accelerator.borrow_mut();
            calls.iter().all(|entry| {
                *map.entry((id, entry.both)).or_insert_with(|| f(&entry.args))
                    == entry.ret
            })
        })
    }

    /// Replay all input-output pairs.
    #[inline]
    pub fn replay<F>(&self, mut f: F)
    where
        F: FnMut(&T),
    {
        for entry in self.0.borrow().iter() {
            if entry.mutable {
                f(&entry.args);
            }
        }
    }
}

impl<T> Default for Constraint<T> {
    fn default() -> Self {
        Self(RefCell::new(vec![]))
    }
}

/// Extend an outer constraint by an inner one.
pub trait Join<T = Self> {
    /// Join this constraint with the `inner` one.
    fn join(&self, inner: &T);

    /// Take out the constraint.
    fn take(&self) -> Self;
}

impl<T: Join> Join<T> for Option<&T> {
    #[inline]
    fn join(&self, inner: &T) {
        if let Some(outer) = self {
            outer.join(inner);
        }
    }

    #[inline]
    fn take(&self) -> Self {
        unimplemented!("cannot call `Join::take` on optional constraint")
    }
}

impl<T: Hash + Clone + PartialEq> Join for Constraint<T> {
    #[inline]
    fn join(&self, inner: &Self) {
        for call in inner.0.borrow().iter() {
            self.push_inner(call.clone());
        }
    }

    #[inline]
    fn take(&self) -> Self {
        Self(RefCell::new(std::mem::take(&mut *self.0.borrow_mut())))
    }
}

/// Produce a 128-bit hash of a value.
#[inline]
pub fn hash<T: Hash>(value: &T) -> u128 {
    let mut state = SipHasher13::new();
    value.hash(&mut state);
    state.finish128().as_u128()
}

/// Check for a constraint violation.
#[inline]
#[track_caller]
#[allow(dead_code)]
fn check(left_hash: u128, right_hash: u128) {
    if left_hash != right_hash {
        panic!(
            "comemo: found conflicting constraints. \
             is this tracked function pure?"
        )
    }
}
