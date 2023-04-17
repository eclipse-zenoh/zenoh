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

use super::keyexpr;

mod classical;
pub use classical::ClassicIntersector;
// #[deprecated = "This module hasn't been updated to support the $* DSL yet"]
// pub(crate) mod ltr;
// #[deprecated = "This module hasn't been updated to support the $* DSL yet"]
// pub(crate) mod ltr_chunk;
// #[deprecated = "This module hasn't been updated to support the $* DSL yet"]
// pub(crate) mod middle_out;
// pub use ltr::LeftToRightIntersector;
// pub use ltr_chunk::LTRChunkIntersector;
// pub use middle_out::MiddleOutIntersector;

pub const DEFAULT_INTERSECTOR: ClassicIntersector = ClassicIntersector;

/// The trait used to implement key expression intersectors.
///
/// Note that `Intersector<&keyexpr, &keyexpr>` is auto-implemented with quickchecks (`streq->true`, `strne&nowild->false`)
/// for any `Intersector<&[u8], &[u8]>`. Implementing `Intersector<&[u8], &[u8]>` is the recommended way to implement intersectors.
pub trait Intersector<Left, Right> {
    fn intersect(&self, left: Left, right: Right) -> bool;
}

pub(crate) mod restiction {
    use core::ops::Deref;

    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct NoBigWilds<T>(pub T);
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct NoSubWilds<T>(pub T);

    impl<T> Deref for NoBigWilds<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
}
#[repr(u8)]
enum MatchComplexity {
    NoWilds = 0,
    ChunkWildsOnly = 1,
    Dsl = 2,
}
trait KeyExprHelpers {
    fn match_complexity(&self) -> MatchComplexity;
}
impl KeyExprHelpers for keyexpr {
    fn match_complexity(&self) -> MatchComplexity {
        let mut has_wilds = false;
        for &c in self.as_bytes() {
            match c {
                b'*' => has_wilds = true,
                b'$' => return MatchComplexity::Dsl,
                _ => {}
            }
        }
        if has_wilds {
            MatchComplexity::ChunkWildsOnly
        } else {
            MatchComplexity::NoWilds
        }
    }
}

use restiction::NoSubWilds;
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
        match left.match_complexity() as u8 | right.match_complexity() as u8 {
            0 => false,
            1 => self.intersect(NoSubWilds(left_bytes), NoSubWilds(right_bytes)),
            _ => self.intersect(left_bytes, right_bytes),
        }
    }
}
