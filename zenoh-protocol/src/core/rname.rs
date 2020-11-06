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
#[inline(always)]
fn cend(s: &str) -> bool {
    s.is_empty() || s.starts_with('/')
}

#[inline(always)]
fn cwild(s: &str) -> bool {
    s.starts_with('*')
}

#[inline(always)]
fn cnext(s: &str) -> &str {
    &s[1..]
}

#[inline(always)]
fn cequal(s1: &str, s2: &str) -> bool {
    s1.starts_with(&s2[0..1])
}

macro_rules! DEFINE_INTERSECT {
    ($name:ident, $end:ident, $wild:ident, $next:ident, $elem_intersect:ident) => {
        fn $name(c1: &str, c2: &str) -> bool {
            if ($end(c1) && $end(c2)) {
                return true;
            }
            if ($wild(c1) && $end(c2)) {
                return $name($next(c1), c2);
            }
            if ($end(c1) && $wild(c2)) {
                return $name(c1, $next(c2));
            }
            if ($wild(c1)) {
                if ($end($next(c1))) {
                    return true;
                }
                if ($name($next(c1), c2)) {
                    return true;
                } else {
                    return $name(c1, $next(c2));
                }
            }
            if ($wild(c2)) {
                if ($end($next(c2))) {
                    return true;
                }
                if ($name($next(c1), c2)) {
                    return true;
                } else {
                    return $name(c1, $next(c2));
                }
            }
            if ($end(c1) || $end(c2)) {
                return false;
            }
            if ($elem_intersect(c1, c2)) {
                return $name($next(c1), $next(c2));
            }
            return false;
        }
    };
}

macro_rules! DEFINE_INCLUDE {
    ($name:ident, $end:ident, $wild:ident, $next:ident, $elem_include:ident) => {
        fn $name(this: &str, sub: &str) -> bool {
            if ($end(this) && $end(sub)) {
                return true;
            }
            if ($wild(this) && $end(sub)) {
                return $name($next(this), sub);
            }
            if ($wild(this)) {
                if ($end($next(this))) {
                    return true;
                }
                if ($name($next(this), sub)) {
                    return true;
                } else {
                    return $name(this, $next(sub));
                }
            }
            if ($wild(sub)) {
                return false;
            }
            if ($end(this) || $end(sub)) {
                return false;
            }
            if ($elem_include(this, sub)) {
                return $name($next(this), $next(sub));
            }
            return false;
        }
    };
}

DEFINE_INTERSECT!(sub_chunk_intersect, cend, cwild, cnext, cequal);

#[inline(always)]
fn chunk_intersect(c1: &str, c2: &str) -> bool {
    if (cend(c1) && !cend(c2)) || (!cend(c1) && cend(c2)) {
        return false;
    }
    sub_chunk_intersect(c1, c2)
}

DEFINE_INCLUDE!(chunk_include, cend, cwild, cnext, cequal);

#[inline(always)]
fn end(s: &str) -> bool {
    s.is_empty()
}

#[inline(always)]
fn wild(s: &str) -> bool {
    s.starts_with("**/") || s == "**"
}

#[inline(always)]
fn next(s: &str) -> &str {
    match s.find('/') {
        Some(idx) => &s[(idx + 1)..],
        None => "",
    }
}

DEFINE_INTERSECT!(res_intersect, end, wild, next, chunk_intersect);

#[inline(always)]
pub fn intersect(s1: &str, s2: &str) -> bool {
    res_intersect(s1, s2)
}

DEFINE_INCLUDE!(res_include, end, wild, next, chunk_include);

#[inline(always)]
pub fn include(this: &str, sub: &str) -> bool {
    res_include(this, sub)
}

pub const ADMIN_PREFIX: &str = "/@/";

#[inline(always)]
pub fn matches(s1: &str, s2: &str) -> bool {
    if s1.starts_with(ADMIN_PREFIX) == s2.starts_with(ADMIN_PREFIX) {
        intersect(s1, s2)
    } else {
        false
    }
}
