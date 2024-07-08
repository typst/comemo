use syn::{
    parse::{Parse, ParseStream},
    token::Token,
};

use super::*;

/// Parse a metadata key-value pair, separated by `=`.
pub fn parse_key_value<K: Token + Default + Parse, V: Parse>(
    input: ParseStream,
) -> Result<Option<V>> {
    if !input.peek(|_| K::default()) {
        return Ok(None);
    }

    let _: K = input.parse()?;
    let _: syn::Token![=] = input.parse()?;
    let value: V = input.parse::<V>()?;
    eat_comma(input);
    Ok(Some(value))
}

/// Parse a comma if there is one.
pub fn eat_comma(input: ParseStream) {
    if input.peek(syn::Token![,]) {
        let _: syn::Token![,] = input.parse().unwrap();
    }
}
