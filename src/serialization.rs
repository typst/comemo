pub use {bincode, once_cell};

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::{collections::HashMap, ops::Deref};

static SERIALIZERS: RwLock<Vec<fn() -> (u128, Vec<u8>)>> = RwLock::new(vec![]);
static LOADERS: RwLock<Lazy<HashMap<u128, fn(&[u8])>>> =
    RwLock::new(Lazy::new(|| HashMap::new()));

pub fn register_serializer(serializer: fn() -> (u128, Vec<u8>)) {
    SERIALIZERS.write().push(serializer)
}

pub fn register_loader(unique_id: u128, loader: fn(&[u8])) {
    let conflict = LOADERS.write().insert(unique_id, loader);
    debug_assert!(conflict.is_none())
}

/// returns a blob of binary data that you may load with [comemo::serialization::deserialize](deserialize).
pub fn serialize() -> bincode::Result<Vec<u8>> {
    bincode::serialize(
        &SERIALIZERS
            .read()
            .deref()
            .iter()
            .map(|f| f())
            .collect::<HashMap<u128, Vec<u8>>>(),
    )
}

/// Errors if data is invalid, in which case the function simply returns without filling the caches.
pub fn deserialize(data: Vec<u8>) -> Result<(), ()> {
    let Ok(data) = bincode::deserialize::<HashMap<u128, Vec<u8>>>(&data) else {
        return Err(());
    };

    for (function_name, load_function) in LOADERS.read().deref().iter() {
        let Some(data) = data.get(function_name) else {
            continue;
        };
        load_function(data)
    }
    Ok(())
}
