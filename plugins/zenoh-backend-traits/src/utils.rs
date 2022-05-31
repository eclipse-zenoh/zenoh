//
// Copyright (c) 2022 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

//! Some useful functions for Backend/Storage implementations.

use zenoh::utils::wire_expr::*;

/// Returns the longest prefix in a key selector that doesn't contain any '*' character.  
/// This would be the common prefix of all keys stored in a storage using this key selector.
///
/// Use this operation at creation of a Storage to get the keys prefix, and in [`Storage::on_sample()`](crate::Storage::on_sample())
/// strip this prefix from all received [`Sample::key_expr`](zenoh::prelude::Sample::key_expr) to retrieve the corrsponding key.
///
/// # Examples:
/// ```
/// # use zenoh_backend_traits::utils::get_keys_prefix;
/// assert_eq!("demo/example/", get_keys_prefix("demo/example/**"));
/// assert_eq!("demo/", get_keys_prefix("demo/**/test/**"));
/// assert_eq!("", get_keys_prefix("**"));
/// assert_eq!("demo/example/test", get_keys_prefix("demo/example/test"));
/// ```
pub fn get_keys_prefix(key_selector: &str) -> &str {
    // keys_prefix is the longest prefix in key_selector without a '*'
    key_selector
        .find('*')
        .map(|i| &key_selector[..i])
        .unwrap_or(key_selector)
}

/// Given a key selector and a prefix (usually returned from [`get_keys_prefix()`]) that is stripped from the keys to store,
/// this operation returns a list of key selectors allowing to match all the keys corresponding to the full keys that would have match
/// the given key selector.
///
/// Use this operation in [`Storage::on_query()`](crate::Storage::on_query()) implementation to transform the received
/// [`Query::selector()`](zenoh::queryable::Query::selector)`.`[`key_selector`](zenoh::prelude::Selector::key_selector) in a list of key selectors
/// that will match all the relevant stored keys (that correspond to keys stripped from the prefix).
///
/// # See also
/// [`get_keys_prefix()`]
///
/// # Examples:
/// ```
/// # use zenoh_backend_traits::utils::get_sub_key_selectors;
/// assert_eq!(
///     ["**"],
///     get_sub_key_selectors("demo/example/test/**", "demo/example/test/").as_slice()
/// );
/// assert_eq!(
///     ["**"],
///     get_sub_key_selectors("demo/example/**", "demo/example/test/").as_slice()
/// );
/// assert_eq!(
///     ["**"],
///     get_sub_key_selectors("**", "demo/example/test/").as_slice()
/// );
/// assert_eq!(
///     ["**/xyz"],
///     get_sub_key_selectors("demo/**/xyz", "demo/example/test/").as_slice()
/// );
/// assert_eq!(
///     ["**"],
///     get_sub_key_selectors("demo/**/test/**", "demo/example/test/").as_slice()
/// );
/// assert_eq!(
///     ["xyz", "**/ex*/*/xyz"],
///     get_sub_key_selectors("demo/**/ex*/*/xyz", "demo/example/test/").as_slice()
/// );
/// ```
pub fn get_sub_key_selectors<'a>(key_selector: &'a str, prefix: &str) -> Vec<&'a str> {
    if let Some(remaining) = key_selector.strip_prefix(prefix) {
        vec![remaining]
    } else {
        let mut result = vec![];
        for (i, c) in key_selector.char_indices().rev() {
            if c == '/' || i == key_selector.len() - 1 {
                let sub_part = &key_selector[..i + 1];
                if intersect(sub_part, prefix) {
                    // if sub_part ends with "**" or "**/", keep those in remaining part
                    let remaining = if sub_part.ends_with("**/") {
                        &key_selector[i - 2..]
                    } else if sub_part.ends_with("**") {
                        &key_selector[i - 1..]
                    } else {
                        &key_selector[i + 1..]
                    };
                    // if remaining is "**" return only this since it covers all
                    if remaining == "**" {
                        return vec!["**"];
                    }
                    result.push(remaining);
                }
            }
        }
        result
    }
}

#[test]
fn test_get_sub_key_exprs() {
    assert_eq!(
        ["**"],
        get_sub_key_selectors("demo/example/test/**", "demo/example/test/").as_slice()
    );
    assert_eq!(
        ["**"],
        get_sub_key_selectors("demo/example/**", "demo/example/test/").as_slice()
    );
    assert_eq!(
        ["**"],
        get_sub_key_selectors("**", "demo/example/test/").as_slice()
    );
    assert_eq!(
        ["**/x*/**"],
        get_sub_key_selectors("demo/example/test/**/x*/**", "demo/example/test/").as_slice()
    );
    assert_eq!(
        ["**/xyz"],
        get_sub_key_selectors("demo/**/xyz", "demo/example/test/").as_slice()
    );
    assert_eq!(
        ["**"],
        get_sub_key_selectors("demo/**/test/**", "demo/example/test/").as_slice()
    );
    assert_eq!(
        ["xyz", "**/ex*/*/xyz"],
        get_sub_key_selectors("demo/**/ex*/*/xyz", "demo/example/test/").as_slice()
    );
    assert_eq!(
        ["xyz", "**/ex*/t*/xyz"],
        get_sub_key_selectors("demo/**/ex*/t*/xyz", "demo/example/test/").as_slice()
    );
    assert_eq!(
        ["*/xyz", "**/te*/*/xyz"],
        get_sub_key_selectors("demo/**/te*/*/xyz", "demo/example/test/").as_slice()
    );
    assert_eq!(
        [""],
        get_sub_key_selectors("demo/example/test", "demo/example/test/").as_slice()
    );
}
