use comemo::{evict, memoize};

#[test]
fn test_evict() {
    empty(); // [Miss]
    empty(); // [Hit]
    evict(2);
    empty(); // [Hit]
    evict(2);
    evict(2);
    empty(); // [Hit]
    evict(2);
    evict(2);
    evict(2);
    empty(); // [Miss]
    empty(); // [Hit]
    evict(0);
    empty(); // [Miss]
    empty(); // [Hit]
}

#[memoize]
fn empty() -> u8 {
    0
}
