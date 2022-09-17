use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;

use siphasher::sip128::{Hasher128, SipHasher};

use crate::constraint::Join;
use crate::input::Input;
use crate::internal::Family;

thread_local! {
    /// The global, dynamic cache shared by all memoized functions.
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

/// Configure the caching behaviour.
pub fn config(config: Config) {
    CACHE.with(|cache| cache.borrow_mut().config = config);
}

/// Configuration for caching behaviour.
pub struct Config {
    max_age: u32,
}

impl Config {
    /// The maximum number of evictions an entry can survive without having been
    /// used in between.
    pub fn max_age(mut self, age: u32) -> Self {
        self.max_age = age;
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self { max_age: 5 }
    }
}

/// Evict cache entries that haven't been used in a while.
pub fn evict() {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let max = cache.config.max_age;
        cache.map.retain(|_, entries| {
            entries.retain_mut(|entry| {
                entry.age += 1;
                entry.age <= max
            });
            !entries.is_empty()
        });
    });
}

/// Execute a function or use a cached result for it.
pub fn memoized<In, Out, F>(id: TypeId, input: In, func: F) -> Out
where
    In: Input,
    Out: Debug + Clone + 'static,
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

        // Check if there is a cached output.
        let mut borrow = cache.borrow_mut();
        if let Some(output) = borrow.lookup::<In, Out>(key, &input) {
            return output;
        }

        borrow.depth += 1;
        drop(borrow);

        // Point all tracked parts of the input to these constraints.
        let constraint = In::Constraint::default();
        let (tracked, outer) = input.retrack(&constraint);

        // Execute the function with the new constraints hooked in.
        let output = func(tracked);

        // Add the new constraints to the previous outer ones.
        outer.join(&constraint);

        // Insert the result into the cache.
        borrow = cache.borrow_mut();
        borrow.insert::<In, Out>(key, constraint, output.clone());
        borrow.depth -= 1;

        output
    })
}

/// The global cache.
#[derive(Default)]
struct Cache {
    /// Maps from function IDs + hashes to memoized results.
    map: HashMap<(TypeId, u128), Vec<Entry>>,
    /// The current depth of the memoized call stack.
    depth: usize,
    /// The current configuration.
    config: Config,
}

impl Cache {
    /// Look for a matching entry in the cache.
    fn lookup<In, Out>(&mut self, key: (TypeId, u128), input: &In) -> Option<Out>
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
    age: u32,
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
    fn lookup<In, Out>(&mut self, input: &In) -> Option<Out>
    where
        In: Input,
        Out: Clone + 'static,
    {
        let Constrained::<In::Constraint, Out> { constraint, output } =
            self.constrained.downcast_ref().expect("wrong entry type");

        input.valid(constraint).then(|| {
            self.age = 0;
            output.clone()
        })
    }
}
