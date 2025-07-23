use std::collections::HashMap;
use std::hash::Hash;

use bumpalo::Bump;
use once_cell::sync::Lazy;
use parking_lot::{Mutex, RwLock};
use siphasher::sip128::{Hasher128, SipHasher13};

use crate::accelerate;
use crate::input::Input;
use crate::internal::Call;
use crate::qtree::{InsertError, LookaheadSequence, QuestionTree};

/// The global list of eviction functions.
static EVICTORS: RwLock<Vec<fn(usize)>> = RwLock::new(Vec::new());

#[cfg(feature = "testing")]
thread_local! {
    /// Whether the last call was a hit.
    static LAST_WAS_HIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub struct Recording<C> {
    immutable: LookaheadSequence<C, u128>,
    mutable: Vec<C>,
}

impl<C> Default for Recording<C> {
    fn default() -> Self {
        Self {
            immutable: LookaheadSequence::new(),
            mutable: Vec::new(),
        }
    }
}

/// Execute a function or use a cached result for it.
pub fn memoized<'c, In, Out, F>(
    mut input: In,
    list: &'c Mutex<Recording<In::Call>>,
    bump: &'c Bump,
    cache: &Cache<In::Call, Out>,
    _enabled: bool,
    func: F,
) -> Out
where
    In: Input + 'c,
    Out: Clone + 'static,
    F: FnOnce(In::Tracked<'c>) -> Out,
{
    // Early bypass if memoization is disabled.
    // Hopefully the compiler will optimize this away, if the condition is constant.
    // if !enabled {
    //     return memoized_disabled(input, constraint, func);
    // }

    // Compute the hash of the input's key part.
    let key = {
        let mut state = SipHasher13::new();
        input.key(&mut state);
        state.finish128().as_u128()
    };

    // Check if there is a cached output.
    if let Some((value, mutable)) = cache.0.read().lookup::<In>(key, &input) {
        #[cfg(feature = "testing")]
        LAST_WAS_HIT.with(|cell| cell.set(true));

        // Replay mutations.
        for call in mutable {
            input.call_mut(call.clone());
        }

        return value.clone();
    }

    // Execute the function with the new constraints hooked in.
    let sink = |call: In::Call, hash: u128| {
        if call.is_mutable() {
            list.lock().mutable.push(call)
        } else {
            list.lock().immutable.push(call, hash)
        }
    };
    let output = func(input.retrack(sink, bump));
    let list = std::mem::take(&mut *list.lock());

    // Insert the result into the cache.
    match cache.0.write().insert::<In>(key, list, output.clone()) {
        Ok(()) => {}
        Err(err) => {
            #[cfg(debug_assertions)]
            panic!("comemo: cached function is non-deterministic ({err:?})");
        }
    }

    #[cfg(feature = "testing")]
    LAST_WAS_HIT.with(|cell| cell.set(false));

    output
}

// fn memoized_disabled<'c, In, Out, F>(
//     input: In,
//     constraint: &'c In::Constraint,
//     func: F,
// ) -> Out
// where
//     In: Input + 'c,
//     Out: Clone + 'static,
//     F: FnOnce(In::Tracked<'c>) -> Out,
// {
//     // Execute the function with the new constraints hooked in.
//     let (input, outer) = input.retrack(constraint);
//     let output = func(input);

//     // Add the new constraints to the outer ones.
//     outer.join(constraint);

//     // Ensure that the last call was a miss during testing.
//     #[cfg(feature = "testing")]
//     LAST_WAS_HIT.with(|cell| cell.set(false));

//     output
// }

/// Evict the global cache.
///
/// This removes all memoized results from the cache whose age is larger than or
/// equal to `max_age`. The age of a result grows by one during each eviction
/// and is reset to zero when the result produces a cache hit. Set `max_age` to
/// zero to completely clear the cache.
pub fn evict(max_age: usize) {
    for subevict in EVICTORS.read().iter() {
        subevict(max_age);
    }

    accelerate::evict();
}

/// Register an eviction function in the global list.
pub fn register_evictor(evict: fn(usize)) {
    EVICTORS.write().push(evict);
}

/// Whether the last call was a hit.
#[cfg(feature = "testing")]
pub fn last_was_hit() -> bool {
    LAST_WAS_HIT.with(|cell| cell.get())
}

/// A cache for a single memoized function.
pub struct Cache<C, Out>(Lazy<RwLock<CacheData<C, Out>>>);

impl<C: 'static, Out: 'static> Cache<C, Out> {
    /// Create an empty cache.
    ///
    /// It must take an initialization function because the `evict` fn
    /// pointer cannot be passed as an argument otherwise the function
    /// passed to `Lazy::new` is a closure and not a function pointer.
    pub const fn new(init: fn() -> RwLock<CacheData<C, Out>>) -> Self {
        Self(Lazy::new(init))
    }

    /// Evict all entries whose age is larger than or equal to `max_age`.
    pub fn evict(&self, _max_age: usize) {
        // self.0.write().evict(max_age)
    }
}

/// The internal data for a cache.
pub struct CacheData<C, Out> {
    /// Maps from hashes to memoized results.
    entries: HashMap<u128, QuestionTree<C, u128, (Out, Vec<C>)>>,
}

impl<C: PartialEq, Out: 'static> CacheData<C, Out> {
    /// Look for a matching entry in the cache.
    fn lookup<In>(&self, key: u128, input: &In) -> Option<&(Out, Vec<C>)>
    where
        In: Input<Call = C>,
        C: Clone + Hash,
    {
        self.entries.get(&key)?.get(|c| input.call(c.clone()))
    }

    /// Insert an entry into the cache.
    #[allow(clippy::extra_unused_type_parameters, reason = "false positive")]
    fn insert<In>(
        &mut self,
        key: u128,
        recording: Recording<C>,
        output: Out,
    ) -> Result<(), InsertError>
    where
        In: Input<Call = C>,
        C: Clone + Hash,
    {
        self.entries
            .entry(key)
            .or_default()
            .insert(recording.immutable, (output, recording.mutable))
    }
}

impl<C, Out> Default for CacheData<C, Out> {
    fn default() -> Self {
        Self { entries: HashMap::new() }
    }
}
