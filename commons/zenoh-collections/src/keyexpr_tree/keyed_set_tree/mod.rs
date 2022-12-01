use std::ptr::NonNull;

use keyed_set::{KeyExtractor, KeyedSet};
use zenoh_protocol_core::key_expr::keyexpr;

use super::*;

pub struct KeyExprTree<
    Weight,
    Children: ChunkMapType<Box<KeyExprTreeNode<Weight, Children>>> = DefaultChunkMapProvider,
> {
    children: Children::Assoc,
}

impl<Weight, Children: ChunkMapType<Box<KeyExprTreeNode<Weight, Children>>>>
    KeyExprTree<Weight, Children>
{
    pub fn new() -> Self {
        KeyExprTree {
            children: Default::default(),
        }
    }
}

impl<Weight, Children: ChunkMapType<Box<KeyExprTreeNode<Weight, Children>>>> IKeyExprTree<Weight>
    for KeyExprTree<Weight, Children>
where
    Weight: 'static,
    Children: 'static,
    Children::Assoc: ChunkMap<Box<KeyExprTreeNode<Weight, Children>>> + 'static,
{
    type Node = KeyExprTreeNode<Weight, Children>;
    fn node(&self, at: &keyexpr) -> Option<&Self::Node> {
        let mut chunks = at.chunks();
        let mut node = self.children.child_at(chunks.next().unwrap())?;
        for chunk in chunks {
            node = node.children.child_at(chunk)?;
        }
        Some(node)
    }
    fn node_mut(&mut self, at: &keyexpr) -> Option<&mut Self::Node> {
        let mut chunks = at.chunks();
        let mut node = self.children.child_at_mut(chunks.next().unwrap())?;
        for chunk in chunks {
            node = node.children.child_at_mut(chunk)?;
        }
        Some(node)
    }

    fn node_mut_or_create(&mut self, at: &keyexpr) -> &mut Self::Node {
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

    type IntersectionItem<'a> = <Self::Intersection<'a> as Iterator>::Item;
    type Intersection<'a> =
        Intersection<'a, Children, Box<KeyExprTreeNode<Weight, Children>>, Weight>;
    fn intersecting_nodes<'a>(&'a self, ke: &'a keyexpr) -> Self::Intersection<'a> {
        Intersection::new(&self.children, ke)
    }
}
mod tree_iter;
use tree_iter::TreeIter;
mod intersection;
use intersection::Intersection;

impl<Weight, Children: ChunkMapType<Box<KeyExprTreeNode<Weight, Children>>>> Default
    for KeyExprTree<Weight, Children>
{
    fn default() -> Self {
        Self {
            children: Default::default(),
        }
    }
}
impl<'a, 'b, T: HasChunk> IEntry<'a, 'b, T>
    for keyed_set::Entry<'a, T, ChunkExtractor, &'b keyexpr>
{
    fn get_or_insert_with<F: FnOnce(&'b keyexpr) -> T>(self, f: F) -> &'a mut T {
        Self::get_or_insert_with(self, f)
    }
}

pub struct DefaultChunkMapProvider;
impl<T: 'static> ChunkMapType<T> for DefaultChunkMapProvider {
    type Assoc = KeyedSet<T, ChunkExtractor>;
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

impl<T: HasChunk + AsNode<T> + AsNodeMut<T>> ChunkMap<T> for KeyedSet<T, ChunkExtractor> {
    fn child_at(&self, chunk: &keyexpr) -> Option<&T> {
        self.get(&chunk)
    }
    fn child_at_mut(&mut self, chunk: &keyexpr) -> Option<&mut T> {
        self.get_mut_unguarded(&chunk)
    }
    type Entry<'a, 'b> = keyed_set::Entry<'a, T, ChunkExtractor, &'b keyexpr> where Self: 'a + 'b, T: 'b;
    fn entry<'a, 'b>(&'a mut self, chunk: &'b keyexpr) -> Self::Entry<'a, 'b>
    where
        Self: 'a + 'b,
        T: 'b,
    {
        self.entry(chunk)
    }

    type IterItem<'a> = &'a T where Self: 'a;
    type Iter<'a> = keyed_set::Iter<'a, T> where Self: 'a;
    fn children<'a>(&'a self) -> Self::Iter<'a>
    where
        Self: 'a,
    {
        self.iter()
    }

    type IterItemMut<'a> = &'a mut T
    where
        Self: 'a;
    type IterMut<'a> = keyed_set::IterMut<'a, T>
    where
        Self: 'a;

    fn children_mut<'a>(&'a mut self) -> Self::IterMut<'a>
    where
        Self: 'a,
    {
        self.iter_mut()
    }
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
