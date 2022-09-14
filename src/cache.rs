use std::any::Any;
use std::cell::RefCell;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use siphasher::sip128::{Hasher128, SipHasher};

use crate::track::{from_parts, to_parts, Track, Trackable, Tracked};

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
        F: for<'f> Fn(<In::Focus as Family<'f>>::Out) -> Out,
    {
        // Compute the hash of the input's key part.
        let hash = {
            let mut state = SipHasher::new();
            input.hash(&mut state);
            state.finish128().as_u128()
        };

        let mut hit = true;
        let output = self.lookup::<In, Out>(hash, &input).unwrap_or_else(|| {
            let constraint = In::Constraint::default();
            let input = input.focus(&constraint);
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

pub trait Input {
    type Constraint: Default + 'static;
    type Focus: for<'f> Family<'f>;

    fn hash<H: Hasher>(&self, state: &mut H);
    fn valid(&self, constraint: &Self::Constraint) -> bool;
    fn focus<'f>(
        self,
        constraint: &'f Self::Constraint,
    ) -> <Self::Focus as Family<'f>>::Out
    where
        Self: 'f;
}

impl<T: Hash> Input for T {
    type Constraint = ();
    type Focus = HashFamily<T>;

    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(self, state);
    }

    fn valid(&self, _: &()) -> bool {
        true
    }

    fn focus<'f>(self, _: &'f ()) -> Self
    where
        Self: 'f,
    {
        self
    }
}

pub struct HashFamily<T>(PhantomData<T>);
impl<T> Family<'_> for HashFamily<T> {
    type Out = T;
}

impl<'a, T: Track> Input for Tracked<'a, T> {
    type Constraint = <T as Trackable>::Constraint;
    type Focus = TrackedFamily<T>;

    fn hash<H: Hasher>(&self, _: &mut H) {}

    fn valid(&self, constraint: &Self::Constraint) -> bool {
        Trackable::valid(to_parts(*self).0, constraint)
    }

    fn focus<'f>(self, constraint: &'f Self::Constraint) -> Tracked<'f, T>
    where
        Self: 'f,
    {
        from_parts(to_parts(self).0, Some(constraint))
    }
}

pub struct TrackedFamily<T>(PhantomData<T>);
impl<'f, T: Track + 'f> Family<'f> for TrackedFamily<T> {
    type Out = Tracked<'f, T>;
}

pub trait Family<'a> {
    /// The surface with lifetime.
    type Out;
}
