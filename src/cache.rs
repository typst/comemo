use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::atomic::AtomicUsize;

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

pub fn write_prescience(mut sink: impl std::io::Write) {
    let locked = PRESCIENCE_WRITE.data.lock();
    let slice: &[u32] = &locked;
    let buf: &[u8] = unsafe {
        std::slice::from_raw_parts(
            slice.as_ptr().cast(),
            slice.len() * (u32::BITS / u8::BITS) as usize,
        )
    };
    sink.write_all(buf).unwrap();
}

struct PrescienceWrite {
    data: Mutex<Vec<u32>>,
}

impl PrescienceWrite {
    fn hit(&self, i: usize) {
        self.data.lock().push(i as u32);
    }

    fn miss(&self) {
        self.data.lock().push(u32::MAX);
    }
}

static PRESCIENCE_WRITE: PrescienceWrite =
    PrescienceWrite { data: Mutex::new(Vec::new()) };

pub fn put_prescience(data: &'static [u8]) {
    let slice: &'static [u32] =
        unsafe { std::slice::from_raw_parts(data.as_ptr().cast(), data.len() / 4) };
    unsafe {
        *PRESCIENCE_READ.data.get() = slice;
    }
}

struct PrescienceRead {
    data: UnsafeCell<&'static [u32]>,
    i: AtomicUsize,
}

impl PrescienceRead {
    fn get(&self) -> Option<u32> {
        let i = self.i.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let slice = unsafe { *self.data.get() };
        if slice.is_empty() {
            return None;
        }
        Some(slice[i])
    }
}

unsafe impl Sync for PrescienceRead {}

static PRESCIENCE_READ: PrescienceRead =
    PrescienceRead { data: UnsafeCell::new(&[]), i: AtomicUsize::new(0) };

/// Execute a function or use a cached result for it.
pub fn memoized<'c, In, Out, F>(
    mut input: In,
    list: &'c Mutex<Recording<In::Call>>,
    bump: &'c Bump,
    cache: &Cache<In::Call, Out>,
    enabled: bool,
    func: F,
) -> Out
where
    In: Input + 'c,
    Out: Clone + 'static,
    F: FnOnce(In::Tracked<'c>) -> Out,
{
    if let Some(i) = PRESCIENCE_READ.get() {
        if i != u32::MAX {
            return cache.0.read().values[i as usize].clone();
        }
        let value = func(input.retrack_noop());
        cache.0.write().values.push(value.clone());
        return value;
    }

    // Compute the hash of the input's key part.
    let key = {
        let mut state = SipHasher13::new();
        input.key(&mut state);
        state.finish128().as_u128()
    };

    // Check if there is a cached output.
    if let Some((i, value, mutable)) = cache.0.read().lookup::<In>(key, &input) {
        PRESCIENCE_WRITE.hit(i);

        #[cfg(feature = "testing")]
        LAST_WAS_HIT.with(|cell| cell.set(true));

        // Replay mutations.
        for call in mutable {
            input.call_mut(call.clone());
        }

        return value.clone();
    }

    PRESCIENCE_WRITE.miss();

    // Execute the function with the new constraints hooked in.
    let sink = |call: In::Call, hash: u128| {
        if call.is_mutable() {
            list.lock().mutable.push(call);
            true
        } else {
            list.lock().immutable.insert(call, hash)
        }
    };
    let output = func(input.retrack(sink, bump));
    let list = std::mem::take(&mut *list.lock());

    // Insert the result into the cache.
    match cache.0.write().insert::<In>(key, list, output.clone()) {
        Ok(()) => {}
        Err(InsertError::AlreadyExists) => {
            // A concurrent call with the same arguments can have inserted
            // a result in the meantime.
        }
        Err(InsertError::WrongQuestion) => {
            #[cfg(debug_assertions)]
            panic!("comemo: cached function is non-deterministic");
        }
    }

    #[cfg(feature = "testing")]
    LAST_WAS_HIT.with(|cell| cell.set(false));

    output
}

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
        // TODO
    }
}

/// The internal data for a cache.
pub struct CacheData<C, Out> {
    /// Maps from hashes to memoized results.
    entries: HashMap<u128, QuestionTree<C, u128, (usize, Vec<C>)>>,
    values: Vec<Out>,
}

impl<C: PartialEq, Out: 'static> CacheData<C, Out> {
    /// Look for a matching entry in the cache.
    fn lookup<In>(&self, key: u128, input: &In) -> Option<(usize, &Out, &Vec<C>)>
    where
        In: Input<Call = C>,
        C: Clone + Hash,
    {
        self.entries
            .get(&key)?
            .get(|c| input.call(c.clone()))
            .map(|(i, c)| (*i, &self.values[*i], c))
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
        let i = self.values.len();
        let res = self
            .entries
            .entry(key)
            .or_default()
            .insert(recording.immutable, (i, recording.mutable));
        if res.is_ok() {
            self.values.push(output);
        }
        res
    }
}

impl<C, Out> Default for CacheData<C, Out> {
    fn default() -> Self {
        Self { entries: HashMap::new(), values: Vec::new() }
    }
}
