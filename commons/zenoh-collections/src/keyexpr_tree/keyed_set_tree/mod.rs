use std::{
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use zenoh_protocol_core::key_expr::keyexpr;

use super::*;

pub struct KeyExprTree<
    Weight,
    Children: ChunkMapType<Box<KeyExprTreeNode<Weight, Children>>> = DefaultChunkMapProvider,
> {
    children: Children::Assoc,
    has_wilds: bool,
}

impl<Weight, Children: ChunkMapType<Box<KeyExprTreeNode<Weight, Children>>>>
    KeyExprTree<Weight, Children>
{
    pub fn new() -> Self {
        KeyExprTree {
            children: Default::default(),
            has_wilds: false,
        }
    }
}

impl<Weight, Children: ChunkMapType<Box<KeyExprTreeNode<Weight, Children>>>> IKeyExprTree<Weight>
    for KeyExprTree<Weight, Children>
where
    Weight: 'static,
    Children: 'static,
    Children::Assoc: ChunkMap<
            Box<KeyExprTreeNode<Weight, Children>>,
            Node = Box<KeyExprTreeNode<Weight, Children>>,
        > + 'static,
{
    type Node = KeyExprTreeNode<Weight, Children>;
    fn node(&self, at: &keyexpr) -> Option<&Self::Node> {
        let mut chunks = at.chunks();
        let mut node = self.children.child_at(chunks.next().unwrap())?;
        for chunk in chunks {
            node = node.as_node().children.child_at(chunk)?;
        }
        Some(node.as_node())
    }
    fn node_mut(&mut self, at: &keyexpr) -> Option<&mut Self::Node> {
        let mut chunks = at.chunks();
        let mut node = self.children.child_at_mut(chunks.next().unwrap())?;
        for chunk in chunks {
            node = node.as_node_mut().children.child_at_mut(chunk)?;
        }
        Some(node.as_node_mut())
    }

    fn node_mut_or_create(&mut self, at: &keyexpr) -> &mut Self::Node {
        if at.is_wild() {
            self.has_wilds = true;
        }
        let mut chunks = at.chunks();
        let root = NonNull::from(&*self);
        let mut node = self
            .children
            .entry(chunks.next().unwrap())
            .get_or_insert_with(move |k| {
                Box::new(KeyExprTreeNode {
                    parent: Parent::Root(root),
                    chunk: k.into(),
                    children: Default::default(),
                    weight: None,
                })
            });
        for chunk in chunks {
            let parent = NonNull::from(node.as_ref());
            node = node.children.entry(chunk).get_or_insert_with(move |k| {
                Box::new(KeyExprTreeNode {
                    parent: Parent::Node(parent),
                    chunk: k.into(),
                    children: Default::default(),
                    weight: None,
                })
            })
        }
        node
    }
    type TreeIterItem<'a> = <Self::TreeIter<'a> as Iterator>::Item;
    type TreeIter<'a> = TreeIter<'a, Children, Box<KeyExprTreeNode<Weight, Children>>, Weight>;
    fn tree_iter(&self) -> Self::TreeIter<'_> {
        TreeIter::new(&self.children)
    }
    type TreeIterItemMut<'a> = <Self::TreeIterMut<'a> as Iterator>::Item;
    type TreeIterMut<'a> =
        TreeIterMut<'a, Children, Box<KeyExprTreeNode<Weight, Children>>, Weight>;
    fn tree_iter_mut(&mut self) -> Self::TreeIterMut<'_> {
        TreeIterMut::new(&mut self.children)
    }

    type IntersectionItem<'a> = <Self::Intersection<'a> as Iterator>::Item;
    type Intersection<'a> = IterOrOption<
        Intersection<'a, Children, Box<KeyExprTreeNode<Weight, Children>>, Weight>,
        &'a Self::Node,
    >;
    fn intersecting_nodes<'a>(&'a self, ke: &'a keyexpr) -> Self::Intersection<'a> {
        if self.has_wilds || ke.is_wild() {
            Intersection::new(&self.children, ke).into()
        } else {
            let node = self.node(ke);
            IterOrOption::Opt(node)
        }
    }

    type IntersectionItemMut<'a> = <Self::IntersectionMut<'a> as Iterator>::Item;
    type IntersectionMut<'a> = IterOrOption<
        IntersectionMut<'a, Children, Box<KeyExprTreeNode<Weight, Children>>, Weight>,
        &'a mut Self::Node,
    >;
    fn intersecting_nodes_mut<'a>(&'a mut self, ke: &'a keyexpr) -> Self::IntersectionMut<'a> {
        if self.has_wilds || ke.is_wild() {
            IntersectionMut::new(&mut self.children, ke).into()
        } else {
            let node = self.node_mut(ke);
            IterOrOption::Opt(node)
        }
    }

    type InclusionItem<'a> = <Self::Inclusion<'a> as Iterator>::Item;
    type Inclusion<'a> = IterOrOption<
        Inclusion<'a, Children, Box<KeyExprTreeNode<Weight, Children>>, Weight>,
        &'a Self::Node,
    >;
    fn included_nodes<'a>(&'a self, ke: &'a keyexpr) -> Self::Inclusion<'a> {
        if self.has_wilds || ke.is_wild() {
            Inclusion::new(&self.children, ke).into()
        } else {
            let node = self.node(ke);
            IterOrOption::Opt(node)
        }
    }

    type InclusionItemMut<'a> = <Self::InclusionMut<'a> as Iterator>::Item;
    type InclusionMut<'a> = IterOrOption<
        InclusionMut<'a, Children, Box<KeyExprTreeNode<Weight, Children>>, Weight>,
        &'a mut Self::Node,
    >;
    fn included_nodes_mut<'a>(&'a mut self, ke: &'a keyexpr) -> Self::InclusionMut<'a> {
        if self.has_wilds || ke.is_wild() {
            InclusionMut::new(&mut self.children, ke).into()
        } else {
            let node = self.node_mut(ke);
            IterOrOption::Opt(node)
        }
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
trait Coerce<Into> {
    fn coerce(self) -> Into;
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
mod tree_iter;
use tree_iter::{TreeIter, TreeIterMut};
mod intersection;
use intersection::{Intersection, IntersectionMut};
mod inclusion;
use inclusion::{Inclusion, InclusionMut};

impl<Weight, Children: ChunkMapType<Box<KeyExprTreeNode<Weight, Children>>>> Default
    for KeyExprTree<Weight, Children>
{
    fn default() -> Self {
        Self {
            children: Default::default(),
            has_wilds: false,
        }
    }
}

pub type DefaultChunkMapProvider = KeyedSetProvider;
pub use keyed_set_impl::KeyedSetProvider;
pub use vec_set_impl::VecSetProvider;
mod keyed_set_impl;
mod vec_set_impl;

pub struct KeyExprTreeNode<Weight, Children: ChunkMapType<Box<Self>>> {
    parent: Parent<Weight, Children>,
    chunk: OwnedKeyExpr,
    children: Children::Assoc,
    weight: Option<Weight>,
}

impl<Weight, Children: ChunkMapType<Box<Self>>> IKeyExprTreeNode<Weight>
    for KeyExprTreeNode<Weight, Children>
where
    Children::Assoc: ChunkMap<Box<Self>>,
{
    type Parent = Self;
    fn parent(&self) -> Option<&Self> {
        match &self.parent {
            Parent::Root(_) => None,
            Parent::Node(node) => Some(unsafe {
                // this is safe, as a mutable reference to the parent was needed to get a mutable reference to this node in the first place.
                node.as_ref()
            }),
        }
    }
    fn parent_mut(&mut self) -> Option<&mut Self> {
        match &mut self.parent {
            Parent::Root(_) => None,
            Parent::Node(node) => Some(unsafe {
                // this is safe, as a mutable reference to the parent was needed to get a mutable reference to this node in the first place.
                node.as_mut()
            }),
        }
    }
    fn keyexpr(&self) -> OwnedKeyExpr {
        unsafe {
            // self._keyexpr is guaranteed to return a valid KE, so no checks are necessary
            OwnedKeyExpr::from_string_unchecked(self._keyexpr(0))
        }
    }
    fn weight(&self) -> Option<&Weight> {
        self.weight.as_ref()
    }
    fn weight_mut(&mut self) -> Option<&mut Weight> {
        self.weight.as_mut()
    }
    fn take_weight(&mut self) -> Option<Weight> {
        self.weight.take()
    }
    fn insert_weight(&mut self, weight: Weight) -> Option<Weight> {
        self.weight.replace(weight)
    }
    type Child = Box<Self>;
    type Children = Children::Assoc;

    fn children(&self) -> &Self::Children {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Self::Children {
        &mut self.children
    }
}
impl<Weight, Children: ChunkMapType<Box<Self>>> KeyExprTreeNode<Weight, Children>
where
    Children::Assoc: ChunkMap<Box<Self>>,
{
    fn _keyexpr(&self, capacity: usize) -> String {
        let s = match self.parent() {
            Some(parent) => parent._keyexpr(capacity + self.chunk.len() + 1) + "/",
            None => String::with_capacity(capacity + self.chunk.len()),
        };
        s + self.chunk.as_str()
    }
}

impl<Weight, Children: ChunkMapType<Box<Self>>> HasChunk for KeyExprTreeNode<Weight, Children> {
    fn chunk(&self) -> &keyexpr {
        &self.chunk
    }
}
impl<Weight, Children: ChunkMapType<Box<Self>>> AsRef<Self> for KeyExprTreeNode<Weight, Children> {
    fn as_ref(&self) -> &Self {
        self
    }
}
impl<Weight, Children: ChunkMapType<Box<Self>>> AsMut<Self> for KeyExprTreeNode<Weight, Children> {
    fn as_mut(&mut self) -> &mut Self {
        self
    }
}

enum Parent<Weight, Children: ChunkMapType<Box<KeyExprTreeNode<Weight, Children>>>> {
    Root(NonNull<KeyExprTree<Weight, Children>>),
    Node(NonNull<KeyExprTreeNode<Weight, Children>>),
}
