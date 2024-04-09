use std::{collections::HashMap, ops::Deref};

pub use bincode;
use once_cell::sync::Lazy;
use parking_lot::RwLock;

static SERIALIZERS: RwLock<Vec<fn() -> (String, Vec<u8>)>> = RwLock::new(vec![]);
static LOADERS: RwLock<Lazy<HashMap<String, fn(&[u8])>>> =
    RwLock::new(Lazy::new(|| HashMap::new()));

pub fn register_serializer(serializer: fn() -> (String, Vec<u8>)) {
    SERIALIZERS.write().push(serializer)
}

pub fn register_loader(function_path: String, loader: fn(&[u8])) {
    let conflict = LOADERS.write().insert(function_path, loader);
    debug_assert!(conflict.is_none())
}

pub fn serialize() -> HashMap<String, Vec<u8>> {
    SERIALIZERS.read().deref().iter().map(|f| f()).collect()
}

pub fn deserialize(data: HashMap<String, Vec<u8>>) {
    for (function_name, load_function) in LOADERS.read().deref().iter() {
        let Some(data) = data.get(function_name) else {
            continue;
        };
        load_function(data)
    }
}
