use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::hash::Hash;

use siphasher::sip128::{Hasher128, SipHasher};

use crate::constraint::Join;
use crate::input::Input;
use crate::internal::Family;

/// Execute a function or use a cached result for it.
pub fn memoized<In, Out, F>(name: &'static str, unique: TypeId, input: In, func: F) -> Out
where
    In: Input,
    Out: Debug + Clone + 'static,
    F: for<'f> FnOnce(<In::Tracked as Family<'f>>::Out) -> Out,
{
    // Compute the hash of the input's key part.
    let hash = {
        let mut state = SipHasher::new();
        unique.hash(&mut state);
        input.key(&mut state);
        state.finish128().as_u128()
    };

    let mut hit = true;
    let output = CACHE.with(|cache| {
        cache.lookup::<In, Out>(hash, &input).unwrap_or_else(|| {
            DEPTH.with(|v| v.set(v.get() + 1));
            let constraint = In::Constraint::default();
            let (tracked, outer) = input.retrack(&constraint);
            let output = func(tracked);
            outer.join(&constraint);
            cache.insert::<In, Out>(hash, Constrained {
                output: output.clone(),
                constraint,
            });
            hit = false;
            DEPTH.with(|v| v.set(v.get() - 1));
            output
        })
    });

    let depth = DEPTH.with(|v| v.get());
    let label = if hit { "[hit]" } else { "[miss]" };
    eprintln!("{depth} {name:<12} {label:<7} {output:?}");

    output
}

thread_local! {
    /// The global, dynamic cache shared by all memoized functions.
    static CACHE: Cache = Cache::default();

    /// The current depth of the memoized call stack.
    static DEPTH: Cell<usize> = Cell::new(0);
}

/// An untyped cache.
#[derive(Default)]
struct Cache {
    map: RefCell<Vec<Entry>>,
}

/// An entry in the cache.
struct Entry {
    hash: u128,
    constrained: Box<dyn Any>,
}

/// A value with a constraint.
struct Constrained<T, C> {
    output: T,
    constraint: C,
}

impl Cache {
    /// Look for a matching entry in the cache.
    fn lookup<In, Out>(&self, hash: u128, input: &In) -> Option<Out>
    where
        In: Input,
        Out: Clone + 'static,
    {
        self.map
            .borrow()
            .iter()
            .filter(|entry| entry.hash == hash)
            .map(|entry| {
                entry
                    .constrained
                    .downcast_ref::<Constrained<Out, In::Constraint>>()
                    .expect("comemo: a hash collision occurred")
            })
            .find(|output| input.valid(&output.constraint))
            .map(|output| output.output.clone())
    }

    /// Insert an entry into the cache.
    fn insert<In, Out>(&self, hash: u128, constrained: Constrained<Out, In::Constraint>)
    where
        In: Input,
        Out: 'static,
    {
        let entry = Entry { hash, constrained: Box::new(constrained) };
        self.map.borrow_mut().push(entry);
    }
}
