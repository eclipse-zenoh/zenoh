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

impl<Weight, Children: ChunkMapType<Box<KeyExprTreeNode<Weight, Children>>>> IKeyExprTree<Weight>
    for KeyExprTree<Weight, Children>
{
    type Node = KeyExprTreeNode<Weight, Children>;
    fn node(&self, at: &keyexpr) -> Option<&Self::Node> {
        let mut chunks = at.chunks();
        let mut node = self.children.child_at(chunks.next().unwrap())?.as_ref();
        for chunk in chunks {
            node = node.children.child_at(chunk)?.as_ref();
        }
        Some(node)
    }
    fn node_mut(&mut self, at: &keyexpr) -> Option<&mut Self::Node> {
        let mut chunks = at.chunks();
        let mut node = self.children.child_at_mut(chunks.next().unwrap())?.as_mut();
        for chunk in chunks {
            node = node.children.child_at_mut(chunk)?.as_mut();
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

    type TreeIter<'a> = TreeIter<'a, Children, KeyExprTreeNode<Weight, Children>, Weight>
    where
        Self: 'a,
        Self::Node: 'a,;
    fn tree_iter<'a>(&'a self) -> Self::TreeIter<'a> {
        TreeIter::new(&self.children)
    }

    // type Intersection<'a> = Box<dyn Iterator<Item = &'a Self::Node> + 'a>
    // where
    //     Self: 'a,
    //     Self::Node: 'a;

    // fn matching_nodes<'a>(&'a self, ke: &'a keyexpr) -> Self::Intersection<'a> {
    //     if ke.is_wild() {
    //         todo!()
    //     } else {
    //         Box::new(self.node(ke).into_iter())
    //     }
    // }

    // type IntersectionMut<'a>= Box<dyn Iterator<Item = &'a mut Self::Node> + 'a>
    // where
    //     Self: 'a,
    //     Self::Node: 'a;

    // fn matching_nodes_mut<'a>(&'a mut self, ke: &'a keyexpr) -> Self::IntersectionMut<'a> {
    //     if ke.is_wild() {
    //         match ke.get_nonwild_prefix() {
    //             Some(prefix) => {
    //                 let Some(root) = self.node_mut(prefix) else {return Box::new(None.into_iter())};
    //                 todo!()
    //             }
    //             None => Box::new(self.tree_iter().filter()),
    //         }
    //     } else {
    //         Box::new(self.node_mut(ke).into_iter())
    //     }
    // }
}

struct TreeIter<'a, Children: ChunkMapType<Box<Node>>, Node: IKeyExprTreeNode<Weight>, Weight>
where
    Children::Assoc: 'a,
{
    iterators: Vec<<Children::Assoc as ChunkMap<Box<Node>>>::Iter<'a>>,
    _marker: std::marker::PhantomData<Weight>,
}

impl<'a, Children: ChunkMapType<Box<Node>>, Node: IKeyExprTreeNode<Weight>, Weight>
    TreeIter<'a, Children, Node, Weight>
where
    Children::Assoc: 'a,
{
    fn new(children: &Children::Assoc) -> Self {
        Self {
            iterators: vec![children.children()],
            _marker: Default::default(),
        }
    }
}

impl<'a, Children: ChunkMapType<Box<Node>>, Node: IKeyExprTreeNode<Weight> + 'a, Weight> Iterator
    for TreeIter<'a, Children, Node, Weight>
where
    Children::Assoc: 'a,
{
    type Item = &'a Node;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.iterators.last_mut()?.next() {
                Some(node) => {
                    let node = node.as_node();
                    todo!()
                }
                None => {
                    self.iterators.pop();
                }
            }
        }
    }
}

impl<Weight, Children: ChunkMapType<Box<KeyExprTreeNode<Weight, Children>>>> Default
    for KeyExprTree<Weight, Children>
{
    fn default() -> Self {
        Self {
            children: Default::default(),
        }
    }
}

pub trait ChunkMapType<T> {
    type Assoc: ChunkMap<T> + Default;
}

pub trait ChunkMap<T> {
    fn child_at<'a, 'b>(&'a self, chunk: &'b keyexpr) -> Option<&'a T>;
    fn child_at_mut<'a, 'b>(&'a mut self, chunk: &'b keyexpr) -> Option<&'a mut T>;
    type Entry<'a, 'b>: IEntry<'a, 'b, T>
    where
        Self: 'a + 'b,
        T: 'b;
    fn entry<'a, 'b>(&'a mut self, chunk: &'b keyexpr) -> Self::Entry<'a, 'b>
    where
        Self: 'a + 'b,
        T: 'b;
    type IterItem<'a>: HasChunk + AsNode<T>
    where
        Self: 'a;
    type Iter<'a>: Iterator<Item = Self::IterItem<'a>>
    where
        Self: 'a;
    fn children<'a>(&'a self) -> Self::Iter<'a>
    where
        Self: 'a;
    type IterItemMut<'a>: HasChunk
    where
        Self: 'a;
    type IterMut<'a>: Iterator<Item = Self::IterItemMut<'a>>
    where
        Self: 'a;
    fn children_mut<'a>(&'a mut self) -> Self::IterMut<'a>
    where
        Self: 'a;
}
pub trait IEntry<'a, 'b, T> {
    fn get_or_insert_with<F: FnOnce(&'b keyexpr) -> T>(self, f: F) -> &'a mut T;
}
impl<'a, 'b, T: HasChunk> IEntry<'a, 'b, T>
    for keyed_set::Entry<'a, T, ChunkExtractor, &'b keyexpr>
{
    fn get_or_insert_with<F: FnOnce(&'b keyexpr) -> T>(self, f: F) -> &'a mut T {
        Self::get_or_insert_with(self, f)
    }
}
pub trait HasChunk {
    fn chunk(&self) -> &keyexpr;
}
impl<'a, T: HasChunk> HasChunk for &'a T {
    fn chunk(&self) -> &keyexpr {
        T::chunk(self)
    }
}
impl<'a, T: HasChunk> HasChunk for &'a mut T {
    fn chunk(&self) -> &keyexpr {
        T::chunk(self)
    }
}
pub trait AsNode<T> {
    fn as_node(&self) -> &T;
}
impl<T: AsRef<U>, U> AsNode<U> for T {
    fn as_node(&self) -> &U {
        self.as_ref()
    }
}
impl<T: AsMut<U>, U> AsNodeMut<U> for T {
    fn as_node_mut(&mut self) -> &mut U {
        self.as_mut()
    }
}
pub trait AsNodeMut<T> {
    fn as_node_mut(&mut self) -> &mut T;
}

pub struct DefaultChunkMapProvider;
impl<T: HasChunk + AsNode<T> + AsNodeMut<T>> ChunkMapType<T> for DefaultChunkMapProvider {
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
{
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
    fn child_at(&self, chunk: &keyexpr) -> Option<&Self> {
        self.children.child_at(chunk).map(|c| c.as_ref())
    }
    fn child_at_mut(&mut self, chunk: &keyexpr) -> Option<&mut Self> {
        self.children.child_at_mut(chunk).map(|c| c.as_mut())
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
}
impl<Weight, Children: ChunkMapType<Box<Self>>> KeyExprTreeNode<Weight, Children> {
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
