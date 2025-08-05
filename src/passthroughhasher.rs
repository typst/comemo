use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};

/// Hash Map that re-uses the u128 as the hash value
pub(crate) type PassthroughHashMap<Key, Value> =
    HashMap<Key, Value, BuildPassthroughHasher>;

#[derive(Copy, Clone, Default)]
pub(crate) struct BuildPassthroughHasher;

#[derive(Default)]
pub(crate) struct PassthroughHasher {
    value: u64,
}

impl Hasher for PassthroughHasher {
    fn finish(&self) -> u64 {
        self.value
    }

    fn write(&mut self, _bytes: &[u8]) {
        unimplemented!("Unsupported operation")
    }

    fn write_u128(&mut self, i: u128) {
        // truncating conversion
        self.value = i as u64;
    }
}

impl BuildHasher for BuildPassthroughHasher {
    type Hasher = PassthroughHasher;
    fn build_hasher(&self) -> PassthroughHasher {
        PassthroughHasher::default()
    }
}
