use std::any::Any;
use std::cmp::{Ord, PartialOrd};
use std::fmt::{self, Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Deref;

use siphasher::sip128::{Hasher128, SipHasher};

/// A wrapper type with precomputed hash.
///
/// This is useful if you want to pass large values of `T` to memoized
/// functions. Especially recursive structures like trees benefit from
/// intermediate prehashed nodes.
///
/// # `Hash` and `Eq`
/// When implementing both `Hash` and `Eq`, the following property [must
/// hold][property]:
/// ```text
/// a == b -> hash(a) == hash(b)
/// ```
/// The inverse implication does not follow from this immediately. However,
/// _comemo_ uses high-quality 128 bit hashes in all places. This reduces the
/// risk of a hash collision to an absolute minimum. Therefore, this trait
/// additionally provides `PartialEq` and `Eq` implementations that compare by
/// hash instead of by value.
///
/// # Notice
/// Note that for a value `v` of type `T`, `hash(v)` is not necessarily equal to
/// `hash(Prehashed::new(v))`. Writing the precomputed hash into a hasher's
/// state produces different output than writing the value's parts directly.
/// However, that seldomly matters as you are typically either dealing with
/// values of type `T` or with values of type `Prehashed<T>`, not a mix of both.
///
/// [property]: https://doc.rust-lang.org/std/hash/trait.Hash.html
#[derive(Copy, Clone)]
pub struct Prehashed<T: ?Sized> {
    /// The precomputed hash.
    hash: u128,
    /// The wrapped item.
    item: T,
}

impl<T: Hash + 'static> Prehashed<T> {
    /// Compute an item's hash and wrap it.
    #[inline]
    pub fn new(item: T) -> Self {
        Self {
            hash: {
                // Also hash the TypeId because the type might be converted
                // through an unsized coercion.
                let mut state = SipHasher::new();
                item.type_id().hash(&mut state);
                item.hash(&mut state);
                state.finish128().as_u128()
            },
            item,
        }
    }

    /// Return the wrapped value.
    #[inline]
    pub fn into_inner(self) -> T {
        self.item
    }
}

impl<T: ?Sized> Deref for Prehashed<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl<T: Hash + 'static> From<T> for Prehashed<T> {
    #[inline]
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: ?Sized> Hash for Prehashed<T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u128(self.hash);
    }
}

impl<T: Debug + ?Sized> Debug for Prehashed<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.item.fmt(f)
    }
}

impl<T: Default + Hash + 'static> Default for Prehashed<T> {
    #[inline]
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: Eq + ?Sized> Eq for Prehashed<T> {}

impl<T: PartialEq + ?Sized> PartialEq for Prehashed<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl<T: Ord + ?Sized> Ord for Prehashed<T> {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.item.cmp(&other.item)
    }
}

impl<T: PartialOrd + ?Sized> PartialOrd for Prehashed<T> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.item.partial_cmp(&other.item)
    }
}
