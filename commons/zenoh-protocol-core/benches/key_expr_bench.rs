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
use criterion::{criterion_group, criterion_main, Criterion};

use rand::SeedableRng;
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
    c.bench_function("bench_keyexpr_matching", |b| {
        use rand::Rng;
        let tlds = ["com", "org", "fr"];
        let sites = (1..10).map(|n| format!("site_{}", n)).collect::<Vec<_>>();
        let rooms = (1..10).map(|n| format!("room_{}", n)).collect::<Vec<_>>();
        let robots = (1..10).map(|n| format!("robot_{}", n)).collect::<Vec<_>>();
        let sensors = [
            "temperature",
            "positition_X",
            "positition_Y",
            "position_Z",
            "battery",
        ];
        use itertools::iproduct;
        let all_existing = iproduct!(tlds, &sites, &rooms, &robots, sensors)
            .map(|(tld, site, room, robot, sensor)| [tld, site, room, robot, sensor])
            .collect::<Vec<_>>();
        fn mk_route([tld, site, room, robot, sensor]: [&str; 5]) -> String {
            format!("/{}/{}/{}/{}/{}", tld, site, room, robot, sensor)
        }
        let mut rng = rand::rngs::StdRng::from_seed([32; 32]);
        let mut routes = vec!["/**".to_owned(), "/*/**".to_owned()];
        routes.push("/**/site_0/**".to_owned());
        routes.push("/**/site_1/**".to_owned());
        routes.push("/**/site_5/**".to_owned());
        routes.push("/**/site_9/**".to_owned());
        for _ in 0..100 {
            let selected_route_id: usize = rng.gen_range(0..all_existing.len());
            let mut selected_route_components = all_existing[selected_route_id];
            routes.push(mk_route(selected_route_components));
            selected_route_components[4] = "*";
            routes.push(mk_route(selected_route_components));
        }
        let all_existing = all_existing.into_iter().map(mk_route).collect::<Vec<_>>();
        b.iter(move || {
            fn count_matches(routes: &[String], matching: &str) -> usize {
                routes
                    .iter()
                    .filter_map(|r| {
                        zenoh_protocol_core::key_expr::intersect(r, matching).then(|| ())
                    })
                    .count()
            }
            count_matches(&routes, "/**");
            count_matches(&routes, "**/room_7/**");
            for route in &all_existing {
                count_matches(&routes, route);
            }
        });
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
