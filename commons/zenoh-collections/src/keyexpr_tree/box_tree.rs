use std::{
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use keyed_set::{KeyExtractor, KeyedSet, KeyedSetGuard};
use zenoh_protocol_core::key_expr::keyexpr;

use super::*;
pub trait ChunkMapType<T> {
    type Assoc: ChunkMap<T>;
}
pub trait RefIter<'a, T> {
    type Iter: Iterator
    where
        <Self::Iter as Iterator>::Item: Child<T>;
    fn iter(&'a self) -> Self::Iter
    where
        <Self::Iter as Iterator>::Item: Child<T>;
}
pub trait ChunkMap<T>: for<'a> RefIter<'a, T> {
    fn child_at<'a, 'b>(&'a self, chunk: &'b keyexpr) -> Option<&'a T>;
    fn child_at_mut<'a, 'b>(&'a mut self, chunk: &'b keyexpr) -> Option<&'a mut T>;
}
pub trait HasChunk {
    fn chunk(&self) -> &keyexpr;
}
pub trait Child<T>: HasChunk {
    fn data(&self) -> &T;
    fn data_mut(&mut self) -> &mut T;
}
pub struct KeyExprTree<Weight, Children: ChunkMapType<Box<KeyExprTreeNode<Weight, Children>>>> {
    children: Children::Assoc,
}
pub struct KeyExprTreeNode<Weight, Children: ChunkMapType<Box<Self>>> {
    parent: Parent<Weight, Children>,
    chunk: OwnedKeyExpr,
    children: Children::Assoc,
    weight: Option<Weight>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ChunkExtractor;
impl<'a, T: HasChunk> KeyExtractor<'a, T> for ChunkExtractor {
    type Key = &'a keyexpr;
    fn extract(&self, from: &'a T) -> Self::Key {
        from.chunk()
    }
}

impl<'a, T: Child<T> + 'a> RefIter<'a, T> for KeyedSet<T, ChunkExtractor> {
    type Iter = keyed_set::Iter<'a, T>;

    fn iter(&'a self) -> Self::Iter {
        self.iter()
    }
}
impl<T: Child<T> + 'static> ChunkMap<T> for KeyedSet<T, ChunkExtractor> {
    fn child_at(&self, chunk: &keyexpr) -> Option<&T> {
        self.get(&chunk)
    }
    fn child_at_mut(&mut self, chunk: &keyexpr) -> Option<&mut T> {
        self.get_mut_unguarded(&chunk)
    }
}

impl<Weight, Children: ChunkMapType<Box<Self>>> KeyExprTreeNode<Weight, Children> {
    pub fn parent(&self) -> Option<&Self> {
        match &self.parent {
            Parent::Root(_) => None,
            Parent::Node(node) => Some(unsafe { node.as_ref() }),
        }
    }
    pub fn parent_mut(&mut self) -> Option<&mut Self> {
        match &mut self.parent {
            Parent::Root(_) => None,
            Parent::Node(node) => Some(unsafe { node.as_mut() }),
        }
    }
    fn _keyexpr(&self, capacity: usize) -> String {
        let s = match self.parent() {
            Some(parent) => parent._keyexpr(capacity + self.chunk.len() + 1) + "/",
            None => String::with_capacity(capacity + self.chunk.len()),
        };
        s + self.chunk.as_str()
    }
    pub fn keyexpr(&self) -> OwnedKeyExpr {
        unsafe { OwnedKeyExpr::from_string_unchecked(self._keyexpr(0)) }
    }
    pub fn weight(&self) -> Option<&Weight> {
        self.weight.as_ref()
    }
    pub fn weight_mut(&mut self) -> Option<&mut Weight> {
        self.weight.as_mut()
    }
    pub fn child_at(&self, chunk: &keyexpr) -> Option<&Self> {
        self.children.child_at(chunk).map(|c| c.as_ref())
    }
    pub fn child_at_mut(&mut self, chunk: &keyexpr) -> Option<&mut Self> {
        self.children.child_at_mut(chunk).map(|c| c.as_mut())
    }
}

impl<Weight, Children: ChunkMapType<Box<Self>>> HasChunk for KeyExprTreeNode<Weight, Children> {
    fn chunk(&self) -> &keyexpr {
        &self.chunk
    }
}
impl<Weight, Children: ChunkMapType<Box<Self>>> Child<Self> for KeyExprTreeNode<Weight, Children> {
    fn data(&self) -> &Self {
        self
    }
    fn data_mut(&mut self) -> &mut Self {
        self
    }
}

enum Parent<Weight, Children: ChunkMapType<Box<KeyExprTreeNode<Weight, Children>>>> {
    Root(NonNull<KeyExprTree<Weight, Children>>),
    Node(NonNull<KeyExprTreeNode<Weight, Children>>),
}
