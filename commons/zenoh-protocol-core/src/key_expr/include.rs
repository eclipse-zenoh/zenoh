use super::{keyexpr, utils::Split, DELIMITER, DOUBLE_WILD, SINGLE_WILD};

pub const DEFAULT_INCLUDER: LTRIncluder = LTRIncluder;

pub trait Includer<Left, Right> {
    /// Returns `true` if the set defined by `left` includes the one defined by `right`
    fn includes(&self, left: Left, right: Right) -> bool;
}

impl<T: for<'a> Includer<&'a [u8], &'a [u8]>> Includer<&keyexpr, &keyexpr> for T {
    fn includes(&self, left: &keyexpr, right: &keyexpr) -> bool {
        let left = left.as_bytes();
        let right = right.as_bytes();
        if left == right {
            return true;
        }
        if let Some(non_wild_prefix_len) = left.iter().position(|&l| l == SINGLE_WILD) {
            if let Some(right) = right.strip_prefix(&left[..non_wild_prefix_len]) {
                let left = &left[non_wild_prefix_len..];
                if let Some(non_wild_suffix_len) = left.iter().rposition(|&l| l == SINGLE_WILD) {
                    if let Some(right) = right.strip_suffix(&left[(non_wild_suffix_len + 1)..]) {
                        return self.includes(&left[..(non_wild_suffix_len + 1)], right);
                    }
                }
            }
            if &left[non_wild_prefix_len..] == DOUBLE_WILD {
                return self.includes(left, right);
            }
        }
        false
    }
}

pub struct LTRIncluder;
impl Includer<&[u8], &[u8]> for LTRIncluder {
    fn includes(&self, mut left: &[u8], mut right: &[u8]) -> bool {
        loop {
            let (lchunk, lrest) = dbg!(left.split_once(&DELIMITER));
            let lempty = lrest.is_empty();
            if dbg!(lchunk == DOUBLE_WILD) {
                if dbg!(lempty) || self.includes(lrest, right) {
                    return true;
                }
                right = right.split_once(&DELIMITER).1;
                if right.is_empty() {
                    return false;
                }
            } else {
                let (rchunk, rrest) = right.split_once(&DELIMITER);
                if !self.non_double_wild_chunk_includes(lchunk, rchunk) {
                    return false;
                }
                let rempty = rrest.is_empty();
                if lempty {
                    return rempty;
                }
                left = lrest;
                right = rrest;
            }
        }
    }
}
impl LTRIncluder {
    fn non_double_wild_chunk_includes(&self, mut lchunk: &[u8], rchunk: &[u8]) -> bool {
        if let Some(leftmost_star) = lchunk.iter().position(|c| *c == SINGLE_WILD) {
            if let Some(rchunk) = rchunk.strip_prefix(&lchunk[..leftmost_star]) {
                lchunk = &lchunk[(leftmost_star + 1)..];
                if let Some(rightmost_star) = lchunk.iter().rposition(|c| *c == SINGLE_WILD) {
                    if let Some(mut rchunk) = rchunk.strip_suffix(&lchunk[(rightmost_star + 1)..]) {
                        lchunk = &lchunk[..rightmost_star];
                        'left: for lneed in lchunk.spliter(&SINGLE_WILD) {
                            while !rchunk.is_empty() {
                                if rchunk.starts_with(lneed) {
                                    continue 'left;
                                }
                                rchunk = &rchunk[1..];
                            }
                            return false;
                        }
                        true
                    } else {
                        false
                    }
                } else {
                    rchunk.ends_with(lchunk)
                }
            } else {
                false
            }
        } else {
            lchunk == rchunk
        }
    }
}
