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
use zenoh::net::protocol::core::rname::intersect;

#[test]
fn rname_test() {
    assert!(intersect("/", "/"));
    assert!(intersect("/a", "/a"));
    assert!(intersect("/a/", "/a"));
    assert!(intersect("/a", "/a/"));
    assert!(intersect("/a/b", "/a/b"));
    assert!(intersect("/*", "/abc"));
    assert!(intersect("/*", "/abc/"));
    assert!(intersect("/*/", "/abc"));
    assert!(!intersect("/*", "/"));
    assert!(!intersect("/*", "xxx"));
    assert!(intersect("/ab*", "/abcd"));
    assert!(intersect("/ab*d", "/abcd"));
    assert!(intersect("/ab*", "/ab"));
    assert!(!intersect("/ab/*", "/ab"));
    assert!(intersect("/a/*/c/*/e", "/a/b/c/d/e"));
    assert!(intersect("/a/*b/c/*d/e", "/a/xb/c/xd/e"));
    assert!(!intersect("/a/*/c/*/e", "/a/c/e"));
    assert!(!intersect("/a/*/c/*/e", "/a/b/c/d/x/e"));
    assert!(!intersect("/ab*cd", "/abxxcxxd"));
    assert!(intersect("/ab*cd", "/abxxcxxcd"));
    assert!(!intersect("/ab*cd", "/abxxcxxcdx"));
    assert!(intersect("/**", "/abc"));
    assert!(intersect("/**", "/a/b/c"));
    assert!(intersect("/**", "/a/b/c/"));
    assert!(intersect("/**/", "/a/b/c"));
    assert!(intersect("/**/", "/"));
    assert!(intersect("/ab/**", "/ab"));
    assert!(intersect("/**/xyz", "/a/b/xyz/d/e/f/xyz"));
    assert!(!intersect("/**/xyz*xyz", "/a/b/xyz/d/e/f/xyz"));
    assert!(intersect("/a/**/c/**/e", "/a/b/b/b/c/d/d/d/e"));
    assert!(intersect("/a/**/c/**/e", "/a/c/e"));
    assert!(intersect("/a/**/c/*/e/*", "/a/b/b/b/c/d/d/c/d/e/f"));
    assert!(!intersect("/a/**/c/*/e/*", "/a/b/b/b/c/d/d/c/d/d/e/f"));
    assert!(!intersect("/ab*cd", "/abxxcxxcdx"));
    assert!(intersect("/x/abc", "/x/abc"));
    assert!(!intersect("/x/abc", "/abc"));
    assert!(intersect("/x/*", "/x/abc"));
    assert!(!intersect("/x/*", "/abc"));
    assert!(!intersect("/*", "/x/abc"));
    assert!(intersect("/x/*", "/x/abc*"));
    assert!(intersect("/x/*abc", "/x/abc*"));
    assert!(intersect("/x/a*", "/x/abc*"));
    assert!(intersect("/x/a*de", "/x/abc*de"));
    assert!(intersect("/x/a*d*e", "/x/a*e"));
    assert!(intersect("/x/a*d*e", "/x/a*c*e"));
    assert!(intersect("/x/a*d*e", "/x/ade"));
    assert!(!intersect("/x/c*", "/x/abc*"));
    assert!(!intersect("/x/*d", "/x/*e"));
}
