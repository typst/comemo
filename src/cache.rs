use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;

use siphasher::sip128::{Hasher128, SipHasher};

use crate::constraint::Join;
use crate::input::Input;
use crate::internal::Family;

thread_local! {
    /// The global, dynamic cache shared by all memoized functions.
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

/// Execute a function or use a cached result for it.
pub fn memoized<In, Out, F>(id: TypeId, mut input: In, func: F) -> Out
where
    In: Input,
    Out: Clone + 'static,
    F: for<'f> FnOnce(<In::Tracked as Family<'f>>::Out) -> Out,
{
    CACHE.with(|cache| {
        // Compute the hash of the input's key part.
        let key = {
            let mut state = SipHasher::new();
            input.key(&mut state);
            let hash = state.finish128().as_u128();
            (id, hash)
        };

        // Point all tracked parts of the input to these constraints.
        let constraint = In::Constraint::default();

        // Check if there is a cached output.
        let mut borrow = cache.borrow_mut();
        if let Some(constrained) = borrow.lookup::<In, Out>(key, &mut input) {
            // Replay the mutations.
            input.replay(&constrained.constraint);

            // Add the cached constraints to the outer ones.
            input.retrack(&constraint).1.join(&constrained.constraint);

            let value = constrained.output.clone();
            borrow.last_was_hit = true;
            return value;
        }

        // Release the borrow so that nested memoized calls can access the
        // cache without panicking.
        drop(borrow);

        // Execute the function with the new constraints hooked in.
        let (input, outer) = input.retrack(&constraint);
        let output = func(input);

        // Add the new constraints to the outer ones.
        outer.join(&constraint);

        // Insert the result into the cache.
        borrow = cache.borrow_mut();
        borrow.insert::<In, Out>(key, constraint, output.clone());
        borrow.last_was_hit = false;

        output
    })
}

/// Whether the last call was a hit.
pub fn last_was_hit() -> bool {
    CACHE.with(|cache| cache.borrow().last_was_hit)
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
}

/// The global cache.
#[derive(Default)]
struct Cache {
    /// Maps from function IDs + hashes to memoized results.
    map: HashMap<(TypeId, u128), Vec<Entry>>,
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
            .push(Entry::new::<In, Out>(constraint, output));
    }
}

/// A memoized result.
struct Entry {
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

impl Entry {
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

        input.valid(&constrained.constraint).then(|| {
            self.age = 0;
            constrained
        })
    }
}
