use std::any::Any;
use std::cell::RefCell;
use std::fmt::Debug;

use siphasher::sip128::{Hasher128, SipHasher};

use crate::input::Input;
use crate::internal::Family;

thread_local! {
    /// The global, dynamic cache shared by all memoized functions.
    pub static CACHE: Cache = Cache::default();
}

/// An untyped cache.
#[derive(Default)]
pub struct Cache {
    map: RefCell<Vec<Entry>>,
}

/// An entry in the cache.
struct Entry {
    hash: u128,
    output: Box<dyn Any>,
}

/// A value with a constraint.
struct Constrained<T, C> {
    value: T,
    constraint: C,
}

impl Cache {
    /// Execute `f` or use a cached result for it.
    pub fn query<In, Out, F>(&self, name: &str, input: In, func: F) -> Out
    where
        In: Input,
        Out: Debug + Clone + 'static,
        F: for<'f> Fn(<In::Hooked as Family<'f>>::Out) -> Out,
    {
        // Compute the hash of the input's key part.
        let hash = {
            let mut state = SipHasher::new();
            input.key(&mut state);
            state.finish128().as_u128()
        };

        let mut hit = true;
        let output = self.lookup::<In, Out>(hash, &input).unwrap_or_else(|| {
            let constraint = In::Constraint::default();
            let input = input.hook_up(&constraint);
            let value = func(input);
            let constrained = Constrained { value: value.clone(), constraint };
            self.insert::<In, Out>(hash, constrained);
            hit = false;
            value
        });

        let label = if hit { "[hit]" } else { "[miss]" };
        eprintln!("{name:<9} {label:<7} {output:?}");

        output
    }

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
                    .output
                    .downcast_ref::<Constrained<Out, In::Constraint>>()
                    .expect("comemo: hash collision")
            })
            .find(|output| input.valid(&output.constraint))
            .map(|output| output.value.clone())
    }

    /// Insert an entry into the cache.
    fn insert<In, Out>(&self, hash: u128, output: Constrained<Out, In::Constraint>)
    where
        In: Input,
        Out: 'static,
    {
        let entry = Entry { hash, output: Box::new(output) };
        self.map.borrow_mut().push(entry);
    }
}
