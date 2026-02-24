//
// Copyright (c) 2023 ZettaScale Technology
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

#[cfg(feature = "unstable")]
mod scenario1;
#[cfg(feature = "unstable")]
mod scenario2;
#[cfg(feature = "unstable")]
mod scenario3;
#[cfg(feature = "unstable")]
mod scenario4;

use std::time::Duration;

use zenoh::{pubsub::Subscriber, Session};
use zenoh_config::WhatAmI;
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct Node {
    c: zenoh::Config,
}

impl Node {
    pub fn new(mode: WhatAmI, id: &str) -> Self {
        let mut c = zenoh::Config::default();
        c.insert_json5("mode", &format!("\"{mode}\"")).unwrap();
        c.insert_json5("id", &format!("\"{id}\"")).unwrap();
        match mode {
            WhatAmI::Router => c
                .insert_json5("listen/endpoints", "[\"tcp/0.0.0.0:7447\"]")
                .unwrap(),
            WhatAmI::Peer => c
                .insert_json5("listen/endpoints", "[\"tcp/0.0.0.0:0\"]")
                .unwrap(),
            _ => (),
        }
        c.insert_json5("scouting/multicast/enabled", "false")
            .unwrap();
        c.insert_json5("adminspace/enabled", "true").unwrap();
        Node { c }
    }

    pub fn insert(mut self, key: &str, value: &str) -> Self {
        self.c.insert_json5(key, value).unwrap();
        self
    }

    pub fn listen(self, listen: &str) -> Self {
        self.insert("listen/endpoints", &format!("[\"{listen}\"]"))
    }

    pub fn connect(self, connect: &[&str]) -> Self {
        self.insert("connect/endpoints", &format!("{connect:?}"))
    }

    pub fn endpoints(self, listen: &str, connect: &[&str]) -> Self {
        self.listen(listen).connect(connect)
    }

    pub fn multicast(self, group: &str) -> Self {
        self.insert("scouting/multicast/enabled", "true")
            .insert("scouting/multicast/address", &format!("\"{group}\""))
    }

    pub fn gateway<S>(self, conf: S) -> Self
    where
        S: AsRef<str>,
    {
        self.insert("gateway", conf.as_ref())
    }

    pub fn region(self, name: &str) -> Self {
        self.insert("region_name", &format!("{name:?}"))
    }

    pub async fn open(self) -> Session {
        ztimeout!(zenoh::open(self.c)).unwrap()
    }
}

#[macro_export]
macro_rules! loc {
    ($session:expr) => {
        ztimeout!($session.info().locators())
            .into_iter()
            .fold(None, |accu, item| match accu {
                None => Some(item),
                Some(loc) => {
                    // Select IPv4 locators prior to IPv6 locators for github CI
                    if loc.as_str().contains("[") {
                        Some(item)
                    } else {
                        Some(loc)
                    }
                }
            })
            .unwrap()
            .as_str()
    };
}

#[macro_export]
macro_rules! count {
    ($storage:expr, $($span:ident{$($k:ident=$v:expr )*}:)* $($p:literal $(,)?)* ) => {
        $storage.all_events().filter(|e| {
            $(
                let span_name = stringify!($span);
                let mut found = false;
                let mut parent = e.parent();
                while let Some(span) = parent {
                    if span.metadata().name() == span_name {
                        $(
                            let key = stringify!($k);
                            if !span.value(key).is_some_and(|v|
                                    v.as_str().is_some_and(|v| v == $v || ($v.ends_with("...") && v.starts_with(&$v[0..$v.len()-3])))
                                    || v.as_debug_str().is_some_and(|v| v == $v || ($v.ends_with("...") && v.starts_with(&$v[0..$v.len()-3])))
                            ) {
                                parent = span.parent();
                                continue
                            }
                        )*
                        found = true;
                        break;
                    }
                    parent = span.parent();
                }

                if !found {
                    return false
                }
            )*

            e.message().is_some_and(|m| {
                true
                    $(&&  m.contains($p))*
            })
        })
        .count()
    }
}

#[macro_export]
macro_rules! json {
    ($($json:tt)+) => {
        serde_json::json!($($json)+).to_string()
    }
}

#[macro_export]
macro_rules! skip_fmt {
    ($($code:tt)+) => {
        $($code)+
    }
}

pub trait SubUtils {
    fn count_keys(&self) -> usize;
    fn count_vals(&self) -> usize;
}

impl SubUtils for Subscriber<flume::Receiver<zenoh::sample::Sample>> {
    fn count_keys(&self) -> usize {
        use itertools::Itertools;
        self.handler()
            .try_iter()
            .map(|s| s.key_expr().clone().into_owned())
            .unique()
            .count()
    }
    fn count_vals(&self) -> usize {
        use itertools::Itertools;
        self.handler()
            .try_iter()
            .map(|s| s.payload().try_to_string().unwrap().into_owned())
            .unique()
            .count()
    }
}

pub mod predicates_ext {
    use std::fmt::Display;

    use predicates::{
        ord::eq, prelude::PredicateBooleanExt, reflection::PredicateReflection, Predicate,
    };
    use tracing_capture::{
        predicates::{ancestor, field, name, IntoFieldPredicate},
        Captured,
    };
    use tracing_tunnel::TracedValue;

    pub fn register_subscriber<'a, C>(zid: &'static str, keyexpr: &'static str) -> impl Predicate<C>
    where
        C: Captured<'a>,
    {
        ancestor(
            name(eq("register_subscriber"))
                & field("self", dbg_obj_eq("north"))
                & field("res", dbg_obj_eq(keyexpr)),
        )
        .and(ancestor(name(eq("demux")) & field("zid", dbg_obj_eq(zid))))
    }

    pub fn register_queryable<'a, C>(zid: &'static str, keyexpr: &'static str) -> impl Predicate<C>
    where
        C: Captured<'a>,
    {
        ancestor(
            name(eq("register_queryable"))
                & field("self", dbg_obj_eq("north"))
                & field("res", dbg_obj_eq(keyexpr)),
        )
        .and(ancestor(name(eq("demux")) & field("zid", dbg_obj_eq(zid))))
    }

    pub fn dbg_obj_eq(matcher: &'static str) -> DebugObjectRegexPredicate {
        DebugObjectRegexPredicate {
            matcher: regex::Regex::new(&regex::escape(matcher)).unwrap(),
        }
    }

    pub fn dbg_obj_re(matcher: regex::Regex) -> DebugObjectRegexPredicate {
        DebugObjectRegexPredicate { matcher }
    }

    #[derive(Debug)]
    pub struct DebugObjectRegexPredicate {
        matcher: regex::Regex,
    }

    impl IntoFieldPredicate for DebugObjectRegexPredicate {
        type Predicate = DebugObjectRegexPredicate;

        fn into_predicate(self) -> Self::Predicate {
            self
        }
    }

    impl Display for DebugObjectRegexPredicate {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{self:?}")
        }
    }

    impl PredicateReflection for DebugObjectRegexPredicate {}

    impl Predicate<TracedValue> for DebugObjectRegexPredicate {
        fn eval(&self, variable: &TracedValue) -> bool {
            match variable {
                TracedValue::Object(obj) => self.matcher.is_match(obj.as_ref()),
                _ => false,
            }
        }
    }
}
