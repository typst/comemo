#[test]
fn test_simple() {
    empty(); // [Miss] The cache is empty.
    empty(); // [Hit] Always a hit from now on.
    empty(); // [Hit] Always a hit from now on.

    double(2); // [Miss] The cache is empty.
    double(4); // [Miss] Different number.
    double(2); // [Hit] Same number as initially.

    sum(2, 4); // [Miss] The cache is empty.
    sum(2, 3); // [Miss] Different numbers.
    sum(2, 3); // [Hit]  Same numbers
    sum(4, 2); // [Miss] Different numbers.
}

/// Build a string.
#[comemo::memoize]
fn empty() -> String {
    format!("The world is {}", "big")
}

/// Double a number.
#[comemo::memoize]
fn double(x: u32) -> u32 {
    2 * x
}

/// Compute the sum of two numbers.
#[comemo::memoize]
fn sum(a: u32, b: u32) -> u32 {
    a + b
}
