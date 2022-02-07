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

use zenoh_protocol_core::key_expr::intersect;
fn run_intersections<const N: usize>(pool: [(&str, &str); N]) {
    for (l, r) in pool {
        intersect(l, r);
    }
}
fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("bench_key_expr_same_str_no_seps", |b| {
        let data = [(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )];
        b.iter(|| {
            run_intersections(data);
        })
    });
    c.bench_function("bench_key_expr_same_str_with_seps", |b| {
        let data = [(
            "/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a",
            "/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a",
        )];
        b.iter(|| {
            run_intersections(data);
        })
    });
    c.bench_function("bench_key_expr_single_star", |b| {
        let data = [("/*", "/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")];
        b.iter(|| run_intersections(data))
    });
    c.bench_function("bench_key_expr_double_star", |b| {
        let data = [(
            "/**",
            "/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a/a",
        )];
        b.iter(|| run_intersections(data))
    });
    c.bench_function("bench_key_expr_many_exprs", |b| {
        let data = [
            ("/", "/"),
            ("/a", "/a"),
            ("/a/", "/a"),
            ("/a", "/a/"),
            ("/a/b", "/a/b"),
            ("/*", "/abc"),
            ("/*", "/abc/"),
            ("/*/", "/abc"),
            ("/*", "/"),
            ("/*", "xxx"),
            ("/ab*", "/abcd"),
            ("/ab*d", "/abcd"),
            ("/ab*", "/ab"),
            ("/ab/*", "/ab"),
            ("/a/*/c/*/e", "/a/b/c/d/e"),
            ("/a/*b/c/*d/e", "/a/xb/c/xd/e"),
            ("/a/*/c/*/e", "/a/c/e"),
            ("/a/*/c/*/e", "/a/b/c/d/x/e"),
            ("/ab*cd", "/abxxcxxd"),
            ("/ab*cd", "/abxxcxxcd"),
            ("/ab*cd", "/abxxcxxcdx"),
            ("/**", "/abc"),
            ("/**", "/a/b/c"),
            ("/**", "/a/b/c/"),
            ("/**/", "/a/b/c"),
            ("/**/", "/"),
            ("/ab/**", "/ab"),
            ("/**/xyz", "/a/b/xyz/d/e/f/xyz"),
            ("/**/xyz*xyz", "/a/b/xyz/d/e/f/xyz"),
            ("/a/**/c/**/e", "/a/b/b/b/c/d/d/d/e"),
            ("/a/**/c/**/e", "/a/c/e"),
            ("/a/**/c/*/e/*", "/a/b/b/b/c/d/d/c/d/e/f"),
            ("/a/**/c/*/e/*", "/a/b/b/b/c/d/d/c/d/d/e/f"),
            ("/ab*cd", "/abxxcxxcdx"),
            ("/x/abc", "/x/abc"),
            ("/x/abc", "/abc"),
            ("/x/*", "/x/abc"),
            ("/x/*", "/abc"),
            ("/*", "/x/abc"),
            ("/x/*", "/x/abc*"),
            ("/x/*abc", "/x/abc*"),
            ("/x/a*", "/x/abc*"),
            ("/x/a*de", "/x/abc*de"),
            ("/x/a*d*e", "/x/a*e"),
            ("/x/a*d*e", "/x/a*c*e"),
            ("/x/a*d*e", "/x/ade"),
            ("/x/c*", "/x/abc*"),
            ("/x/*d", "/x/*e"),
        ];
        b.iter(|| run_intersections(data))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
