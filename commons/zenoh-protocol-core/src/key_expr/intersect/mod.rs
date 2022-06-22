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

use super::keyexpr;

trait Utf {
    fn utf(&self) -> &str;
}
impl Utf for [u8] {
    fn utf(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(self) }
    }
}
macro_rules! utfdbg {
    ($x: expr) => {{
        let x = $x;
        println!("[{}:{}] {} = {}", file!(), line!(), stringify!($x), x.utf());
        x
    }};
}
pub(crate) mod classic_matcher {
    use crate::key_expr::keyexpr;

    use super::Intersector;

    pub struct ClassicMatcher;
    impl Intersector<&keyexpr, &keyexpr> for ClassicMatcher {
        fn intersect(&self, left: &keyexpr, right: &keyexpr) -> bool {
            crate::wire_expr::intersect(left, right)
        }
    }
}
pub(crate) mod ltr;
pub(crate) mod ltr_chunk;
pub(crate) mod middle_out;
pub use ltr::LeftToRightIntersector;
pub use ltr_chunk::LTRChunkIntersector;
pub use middle_out::MiddleOutIntersector;

pub const DEFAULT_INTERSECTOR: MiddleOutIntersector<LTRChunkIntersector> =
    MiddleOutIntersector(LTRChunkIntersector);

/// The trait used to implement key expression intersectors.
///
/// Note that `Intersector<&keyexpr, &keyexpr>` is auto-implemented with quickchecks (`streq->true`, `strne&nowild->false`)
/// for any `Intersector<&[u8], &[u8]>`. Implementing `Intersector<&[u8], &[u8]>` is the recommended way to implement intersectors.
pub trait Intersector<Left, Right> {
    fn intersect(&self, left: Left, right: Right) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NoBigWilds<T>(pub T);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NoSubWilds<T>(pub T);
impl<T> std::ops::Deref for NoBigWilds<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

enum MatchComplexity {
    NoWilds,
    ChunkWildsOnly,
    SubChunkWilds,
}
trait KeyExprHelpers {
    fn match_complexity(&self) -> MatchComplexity;
}
impl KeyExprHelpers for keyexpr {
    fn match_complexity(&self) -> MatchComplexity {
        let mut has_wilds = false;
        let mut previous_char = b'/';
        for &char in self.as_bytes() {
            match (previous_char, char) {
                (b'*', b'/') | (b'/', b'*') | (b'*', b'*') => has_wilds = true,
                (b'*', _) | (_, b'*') => return MatchComplexity::SubChunkWilds,
                _ => {}
            }
            previous_char = char;
        }
        if has_wilds {
            MatchComplexity::ChunkWildsOnly
        } else {
            MatchComplexity::NoWilds
        }
    }
}

impl<
        T: for<'a> Intersector<&'a [u8], &'a [u8]>
            + for<'a> Intersector<NoSubWilds<&'a [u8]>, NoSubWilds<&'a [u8]>>,
    > Intersector<&keyexpr, &keyexpr> for T
{
    fn intersect(&self, left: &keyexpr, right: &keyexpr) -> bool {
        let left_bytes = left.as_bytes();
        let right_bytes = right.as_bytes();
        if left_bytes == right_bytes {
            return true;
        }
        match (left.match_complexity(), right.match_complexity()) {
            (MatchComplexity::NoWilds, MatchComplexity::NoWilds) => false,
            (MatchComplexity::SubChunkWilds, _) | (_, MatchComplexity::SubChunkWilds) => {
                self.intersect(left_bytes, right_bytes)
            }
            _ => self.intersect(NoSubWilds(left_bytes), NoSubWilds(right_bytes)),
        }
    }
}
