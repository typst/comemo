use std::any::{Any, TypeId};
use std::sync::atomic::{AtomicUsize, Ordering};

use siphasher::sip128::{Hasher128, SipHasher13};

use crate::constraint::Join;
use crate::input::Input;

/// Execute a function or use a cached result for it.
pub fn memoized<'c, In, Out, F>(
    id: TypeId,
    mut input: In,
    constraint: &'c In::Constraint,
    func: F,
) -> Out
where
    In: Input + 'c,
    Out: Clone + Send + Sync + 'static,
    F: FnOnce(In::Tracked<'c>) -> Out,
{
    // Compute the hash of the input's key part.
    let key = {
        let mut state = SipHasher13::new();
        input.key(&mut state);
        let hash = state.finish128().as_u128();
        (id, hash)
    };

    // Check if there is a cached output.
    if let Some(constrained) = crate::CACHE.get(&key).and_then(|entries| {
        entries
            .try_map(|value| {
                value.iter().rev().find_map(|entry| entry.lookup::<In, Out>(&input))
            })
            .ok()
    }) {
        // Replay the mutations.
        input.replay(&constrained.constraint);

        // Add the cached constraints to the outer ones.
        input.retrack(constraint).1.join(&constrained.constraint);

        let value = constrained.output.clone();
        crate::LAST_WAS_HIT.with(|hit| hit.set(true));
        return value;
    }

    // Execute the function with the new constraints hooked in.
    let (input, outer) = input.retrack(constraint);
    let output = func(input);

    // Add the new constraints to the outer ones.
    outer.join(constraint);

    // Insert the result into the cache.
    crate::LAST_WAS_HIT.with(|cell| cell.set(false));
    crate::CACHE
        .entry(key)
        .or_default()
        .push(CacheEntry::new::<In, Out>(constraint.take(), output.clone()));

    output
}

/// Whether the last call was a hit.
pub fn last_was_hit() -> bool {
    crate::LAST_WAS_HIT.with(|cell| cell.get())
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
    crate::CACHE.retain(|_, entries| {
        entries.retain_mut(|entry| {
            let age = entry.age.fetch_add(1, Ordering::Relaxed);
            age < max_age
        });
        !entries.is_empty()
    });
    crate::ACCELERATOR.clear();
    crate::ID.store(0, Ordering::SeqCst);
}

/// A memoized result.
pub struct CacheEntry {
    /// The memoized function's constrained output.
    ///
    /// This is of type `Constrained<In::Constraint, Out>`.
    constrained: Box<dyn Any + Send + Sync>,
    /// How many evictions have passed since the entry has been last used.
    age: AtomicUsize,
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
        Out: Send + Sync + 'static,
    {
        Self {
            constrained: Box::new(Constrained { constraint, output }),
            age: AtomicUsize::new(0),
        }
    }

    /// Return the entry's output if it is valid for the given input.
    fn lookup<In, Out>(&self, input: &In) -> Option<&Constrained<In::Constraint, Out>>
    where
        In: Input,
        Out: Clone + 'static,
    {
        let constrained: &Constrained<In::Constraint, Out> =
            self.constrained.downcast_ref().expect("wrong entry type");

        input.validate(&constrained.constraint).then(|| {
            self.age.store(0, Ordering::Relaxed);
            constrained
        })
    }
}
