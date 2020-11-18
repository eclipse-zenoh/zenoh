//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//

//! Some useful functions for Backend/Storage implementations.

use zenoh::net::utils::resource_name::*;

/// Returns the longest prefix in a Path expressions that doesn't contain any '*' character.  
/// This would be the common prefix of all keys stored in a storage using this Path expression.
///
/// Use this operation at creation of a Storage to get the keys prefix, and in [`Storage::on_sample()`](crate::Storage::on_sample())
/// strip this prefix from all received [`Sample::res_name`](zenoh::net::Sample::res_name) to retrieve the corrsponding key.
///
/// # Examples:
/// ```
/// # use zenoh_backend_traits::utils::get_keys_prefix;
/// assert_eq!("/demo/example/", get_keys_prefix("/demo/example/**"));
/// assert_eq!("/demo/", get_keys_prefix("/demo/**/test/**"));
/// assert_eq!("/", get_keys_prefix("/**"));
/// assert_eq!("/demo/example/test", get_keys_prefix("/demo/example/test"));
/// ```
pub fn get_keys_prefix(path_expr: &str) -> &str {
    // keys_prefix is the longest prefix in path_expr without a '*'
    path_expr
        .find('*')
        .map(|i| &path_expr[..i])
        .unwrap_or(path_expr)
}

/// Given a Path Expression and a prefix (usually returned from [`get_keys_prefix()`]) that is stripped from the paths to store,
/// this operation returns a list of Path Expr allowing to match all the keys corresponding to the full paths that would have match
/// the given Path Expr.
///
/// Use this operation in [`Storage::on_query()`](crate::Storage::on_query()) implementation to transform the received [`Query::res_name`](zenoh::net::Query::res_name) in a list of path
/// expressions that will match all the relevant stored keys (that correspond to paths stripped from the prefix).
///
/// # See also
/// [`get_keys_prefix()`]
///
/// # Examples:
/// ```
/// # use zenoh_backend_traits::utils::get_sub_path_exprs;
/// assert_eq!(
///     ["**"],
///     get_sub_path_exprs("/demo/example/test/**", "/demo/example/test/").as_slice()
/// );
/// assert_eq!(
///     ["**"],
///     get_sub_path_exprs("/demo/example/**", "/demo/example/test/").as_slice()
/// );
/// assert_eq!(
///     ["**"],
///     get_sub_path_exprs("/**", "/demo/example/test/").as_slice()
/// );
/// assert_eq!(
///     ["**/xyz"],
///     get_sub_path_exprs("/demo/**/xyz", "/demo/example/test/").as_slice()
/// );
/// assert_eq!(
///     ["**"],
///     get_sub_path_exprs("/demo/**/test/**", "/demo/example/test/").as_slice()
/// );
/// assert_eq!(
///     ["xyz", "**/ex*/*/xyz"],
///     get_sub_path_exprs("/demo/**/ex*/*/xyz", "/demo/example/test/").as_slice()
/// );
/// ```
pub fn get_sub_path_exprs<'a>(path_expr: &'a str, prefix: &str) -> Vec<&'a str> {
    if let Some(remaining) = path_expr.strip_prefix(prefix) {
        vec![remaining]
    } else {
        let mut result = vec![];
        for (i, c) in path_expr.char_indices().rev() {
            if c == '/' || i == path_expr.len() - 1 {
                let sub_part = &path_expr[..i + 1];
                if intersect(sub_part, prefix) {
                    // if sub_part ends with "**" or "**/", keep those in remaining part
                    let remaining = if sub_part.ends_with("**/") {
                        &path_expr[i - 2..]
                    } else if sub_part.ends_with("**") {
                        &path_expr[i - 1..]
                    } else {
                        &path_expr[i + 1..]
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
fn test_get_sub_path_exprs() {
    assert_eq!(
        ["**"],
        get_sub_path_exprs("/demo/example/test/**", "/demo/example/test/").as_slice()
    );
    assert_eq!(
        ["**"],
        get_sub_path_exprs("/demo/example/**", "/demo/example/test/").as_slice()
    );
    assert_eq!(
        ["**"],
        get_sub_path_exprs("/**", "/demo/example/test/").as_slice()
    );
    assert_eq!(
        ["**/x*/**"],
        get_sub_path_exprs("/demo/example/test/**/x*/**", "/demo/example/test/").as_slice()
    );
    assert_eq!(
        ["**/xyz"],
        get_sub_path_exprs("/demo/**/xyz", "/demo/example/test/").as_slice()
    );
    assert_eq!(
        ["**"],
        get_sub_path_exprs("/demo/**/test/**", "/demo/example/test/").as_slice()
    );
    assert_eq!(
        ["xyz", "**/ex*/*/xyz"],
        get_sub_path_exprs("/demo/**/ex*/*/xyz", "/demo/example/test/").as_slice()
    );
    assert_eq!(
        ["xyz", "**/ex*/t*/xyz"],
        get_sub_path_exprs("/demo/**/ex*/t*/xyz", "/demo/example/test/").as_slice()
    );
    assert_eq!(
        ["*/xyz", "**/te*/*/xyz"],
        get_sub_path_exprs("/demo/**/te*/*/xyz", "/demo/example/test/").as_slice()
    );
    assert_eq!(
        [""],
        get_sub_path_exprs("/demo/example/test", "/demo/example/test/").as_slice()
    );
}
