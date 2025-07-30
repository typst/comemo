use std::cell::Cell;

thread_local! {
    /// Whether the last call was a cache hit.
    static LAST_WAS_HIT: Cell<bool> = const { Cell::new(false) };
}

/// Whether the last call was a hit.
pub fn last_was_hit() -> bool {
    LAST_WAS_HIT.with(|cell| cell.get())
}

/// Marks the last call as a cache hit.
pub(crate) fn register_hit() {
    LAST_WAS_HIT.with(|cell| cell.set(true))
}

/// Marks the last call as a cache miss.
pub(crate) fn register_miss() {
    LAST_WAS_HIT.with(|cell| cell.set(false))
}
