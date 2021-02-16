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
#[macro_use]
extern crate criterion;

use criterion::Criterion;

use zenoh::net::protocol::core::rname::intersect;

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("bench_rname_1", |b| {
        b.iter(|| {
            intersect(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            );
        })
    });
    c.bench_function("bench_rname_2", |b| {
        b.iter(|| {
            intersect(
                "/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a",
                "/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a",
            );
        })
    });
    c.bench_function("bench_rname_3", |b| {
        b.iter(|| {
            intersect("/*", "/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        })
    });
    c.bench_function("bench_rname_4", |b| {
        b.iter(|| {
            intersect(
                "/**",
                "/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a",
            );
        })
    });
    c.bench_function("bench_rname_5", |b| {
        b.iter(|| {
            intersect("/", "/");
            intersect("/a", "/a");
            intersect("/a/", "/a");
            intersect("/a", "/a/");
            intersect("/a/b", "/a/b");
            intersect("/*", "/abc");
            intersect("/*", "/abc/");
            intersect("/*/", "/abc");
            intersect("/*", "/");
            intersect("/*", "xxx");
            intersect("/ab*", "/abcd");
            intersect("/ab*d", "/abcd");
            intersect("/ab*", "/ab");
            intersect("/ab/*", "/ab");
            intersect("/a/*/c/*/e", "/a/b/c/d/e");
            intersect("/a/*b/c/*d/e", "/a/xb/c/xd/e");
            intersect("/a/*/c/*/e", "/a/c/e");
            intersect("/a/*/c/*/e", "/a/b/c/d/x/e");
            intersect("/ab*cd", "/abxxcxxd");
            intersect("/ab*cd", "/abxxcxxcd");
            intersect("/ab*cd", "/abxxcxxcdx");
            intersect("/**", "/abc");
            intersect("/**", "/a/b/c");
            intersect("/**", "/a/b/c/");
            intersect("/**/", "/a/b/c");
            intersect("/**/", "/");
            intersect("/ab/**", "/ab");
            intersect("/**/xyz", "/a/b/xyz/d/e/f/xyz");
            intersect("/**/xyz*xyz", "/a/b/xyz/d/e/f/xyz");
            intersect("/a/**/c/**/e", "/a/b/b/b/c/d/d/d/e");
            intersect("/a/**/c/**/e", "/a/c/e");
            intersect("/a/**/c/*/e/*", "/a/b/b/b/c/d/d/c/d/e/f");
            intersect("/a/**/c/*/e/*", "/a/b/b/b/c/d/d/c/d/d/e/f");
            intersect("/ab*cd", "/abxxcxxcdx");
            intersect("/x/abc", "/x/abc");
            intersect("/x/abc", "/abc");
            intersect("/x/*", "/x/abc");
            intersect("/x/*", "/abc");
            intersect("/*", "/x/abc");
            intersect("/x/*", "/x/abc*");
            intersect("/x/*abc", "/x/abc*");
            intersect("/x/a*", "/x/abc*");
            intersect("/x/a*de", "/x/abc*de");
            intersect("/x/a*d*e", "/x/a*e");
            intersect("/x/a*d*e", "/x/a*c*e");
            intersect("/x/a*d*e", "/x/ade");
            intersect("/x/c*", "/x/abc*");
            intersect("/x/*d", "/x/*e");
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
