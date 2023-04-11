use alloc::boxed::Box;
use core::ops::{Deref, DerefMut};

/// Allows specializing KeTrees based on their eventual storage of wild KEs.
///
/// This is useful to allow KeTrees to be much faster at iterating when the queried KE is non-wild,
/// and the KeTree is informed by its wildness that it doesn't contain any wilds.
pub trait IWildness: 'static {
    fn non_wild() -> Self;
    fn get(&self) -> bool;
    fn set(&mut self, wildness: bool) -> bool;
}
/// Disallows the KeTree from storing _any_ wild KE.
///
/// Attempting to store a wild KE on a `KeTree<NonWild>` will cause a panic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NonWild;
impl IWildness for NonWild {
    fn non_wild() -> Self {
        NonWild
    }
    fn get(&self) -> bool {
        false
    }
    fn set(&mut self, wildness: bool) -> bool {
        if wildness {
            panic!("Attempted to set NonWild to wild, which breaks its contract. You likely attempted to insert a wild key into a `NonWild` tree. Use `bool` instead to make wildness determined at runtime.")
        }
        false
    }
}
/// A ZST that forces the KeTree to always believe it contains wild KEs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnknownWildness;
impl IWildness for UnknownWildness {
    fn non_wild() -> Self {
        UnknownWildness
    }
    fn get(&self) -> bool {
        true
    }
    fn set(&mut self, _wildness: bool) -> bool {
        true
    }
}
/// Stores the wildness of the KeTree at runtime, allowing it to select its
/// iteration algorithms based on the wildness at the moment of query.
impl IWildness for bool {
    fn non_wild() -> Self {
        false
    }
    fn get(&self) -> bool {
        *self
    }
    fn set(&mut self, wildness: bool) -> bool {
        core::mem::replace(self, wildness)
    }
}

pub enum IterOrOption<Iter: Iterator, Item> {
    Opt(Option<Item>),
    Iter(Iter),
}
impl<Iter: Iterator, Item> Iterator for IterOrOption<Iter, Item>
where
    Iter::Item: Coerce<Item>,
{
    type Item = Item;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IterOrOption::Opt(v) => v.take(),
            IterOrOption::Iter(it) => it.next().map(Coerce::coerce),
        }
    }
}
pub struct Coerced<Iter: Iterator, Item> {
    iter: Iter,
    _item: core::marker::PhantomData<Item>,
}

impl<Iter: Iterator, Item> Coerced<Iter, Item> {
    pub fn new(iter: Iter) -> Self {
        Self {
            iter,
            _item: Default::default(),
        }
    }
}
impl<Iter: Iterator, Item> Iterator for Coerced<Iter, Item>
where
    Iter::Item: Coerce<Item>,
{
    type Item = Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(Coerce::coerce)
    }
}

trait Coerce<Into> {
    fn coerce(self) -> Into;
}
impl<T> Coerce<T> for T {
    fn coerce(self) -> T {
        self
    }
}
impl<'a, T> Coerce<&'a T> for &'a Box<T> {
    fn coerce(self) -> &'a T {
        self.deref()
    }
}
impl<'a, T> Coerce<&'a mut T> for &'a mut Box<T> {
    fn coerce(self) -> &'a mut T {
        self.deref_mut()
    }
}
impl<Iter: Iterator, Item> From<Iter> for IterOrOption<Iter, Item> {
    fn from(it: Iter) -> Self {
        Self::Iter(it)
    }
}
