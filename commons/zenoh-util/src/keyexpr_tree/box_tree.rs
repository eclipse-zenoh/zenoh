use std::{
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::keyexpr_tree::*;
use zenoh_protocol::core::key_expr::keyexpr;

use super::impls::KeyedSetProvider;

#[repr(C)]
pub struct KeBoxTree<
    Weight,
    Wildness: IWildness = bool,
    Children: IChildrenProvider<Box<KeyExprTreeNode<Weight, Wildness, Children>>> = DefaultChildrenProvider,
> {
    children: Children::Assoc,
    wildness: Wildness,
}

impl<
        Weight,
        Wildness: IWildness,
        Children: IChildrenProvider<Box<KeyExprTreeNode<Weight, Wildness, Children>>>,
    > KeBoxTree<Weight, Wildness, Children>
{
    pub fn new() -> Self {
        KeBoxTree {
            children: Default::default(),
            wildness: Wildness::non_wild(),
        }
    }
}
impl<
        Weight,
        Children: IChildrenProvider<Box<KeyExprTreeNode<Weight, Wildness, Children>>>,
        Wildness: IWildness,
    > Default for KeBoxTree<Weight, Wildness, Children>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<
        'a,
        Weight,
        Children: IChildrenProvider<Box<KeyExprTreeNode<Weight, Wildness, Children>>>,
        Wildness: IWildness,
    > IKeyExprTree<'a, Weight> for KeBoxTree<Weight, Wildness, Children>
where
    Weight: 'a,
    Children: 'a,
    Children::Assoc: IChildren<
            Box<KeyExprTreeNode<Weight, Wildness, Children>>,
            Node = Box<KeyExprTreeNode<Weight, Wildness, Children>>,
        > + 'a,
{
    type Node = KeyExprTreeNode<Weight, Wildness, Children>;
    fn node(&'a self, at: &keyexpr) -> Option<&'a Self::Node> {
        let mut chunks = at.chunks();
        let mut node = self.children.child_at(chunks.next().unwrap())?;
        for chunk in chunks {
            node = node.as_node().children.child_at(chunk)?;
        }
        Some(node.as_node())
    }
    type TreeIterItem = <Self::TreeIter as Iterator>::Item;
    type TreeIter =
        TreeIter<'a, Children, Box<KeyExprTreeNode<Weight, Wildness, Children>>, Weight>;
    fn tree_iter(&'a self) -> Self::TreeIter {
        TreeIter::new(&self.children)
    }
    type IntersectionItem = <Self::Intersection as Iterator>::Item;
    type Intersection = IterOrOption<
        Intersection<'a, Children, Box<KeyExprTreeNode<Weight, Wildness, Children>>, Weight>,
        &'a Self::Node,
    >;
    fn intersecting_nodes(&'a self, ke: &'a keyexpr) -> Self::Intersection {
        if self.wildness.get() || ke.is_wild() {
            Intersection::new(&self.children, ke).into()
        } else {
            let node = self.node(ke);
            IterOrOption::Opt(node)
        }
    }

    type InclusionItem = <Self::Inclusion as Iterator>::Item;
    type Inclusion = IterOrOption<
        Inclusion<'a, Children, Box<KeyExprTreeNode<Weight, Wildness, Children>>, Weight>,
        &'a Self::Node,
    >;
    fn included_nodes(&'a self, ke: &'a keyexpr) -> Self::Inclusion {
        if self.wildness.get() || ke.is_wild() {
            Inclusion::new(&self.children, ke).into()
        } else {
            let node = self.node(ke);
            IterOrOption::Opt(node)
        }
    }
}
impl<
        'a,
        Weight,
        Children: IChildrenProvider<Box<KeyExprTreeNode<Weight, Wildness, Children>>>,
        Wildness: IWildness,
    > IKeyExprTreeMut<'a, Weight> for KeBoxTree<Weight, Wildness, Children>
where
    Weight: 'a,
    Children: 'a,
    Children::Assoc: IChildren<
            Box<KeyExprTreeNode<Weight, Wildness, Children>>,
            Node = Box<KeyExprTreeNode<Weight, Wildness, Children>>,
        > + 'a,
{
    fn node_mut<'b>(&'b mut self, at: &keyexpr) -> Option<&'b mut Self::Node> {
        let mut chunks = at.chunks();
        let mut node = self.children.child_at_mut(chunks.next().unwrap())?;
        for chunk in chunks {
            node = node.as_node_mut().children.child_at_mut(chunk)?;
        }
        Some(node.as_node_mut())
    }

    fn remove(&mut self, at: &keyexpr) -> Option<Weight> {
        let node = self.node_mut(at)?;
        if !node.children.is_empty() {
            node.weight.take()
        } else {
            let chunk = unsafe { std::mem::transmute::<_, &keyexpr>(node.chunk()) };
            match node.parent {
                None => &mut self.children,
                Some(parent) => unsafe { &mut (*parent.as_ptr()).children },
            }
            .remove(chunk)
            .and_then(|node| node.weight)
        }
    }

    fn node_mut_or_create<'b>(&'b mut self, at: &keyexpr) -> &'b mut Self::Node {
        if at.is_wild() {
            self.wildness.set(true);
        }
        let mut chunks = at.chunks();
        let mut node = self
            .children
            .entry(chunks.next().unwrap())
            .get_or_insert_with(move |k| {
                Box::new(KeyExprTreeNode {
                    parent: None,
                    chunk: k.into(),
                    children: Default::default(),
                    weight: None,
                })
            });
        for chunk in chunks {
            let parent = NonNull::from(node.as_ref());
            node = node.children.entry(chunk).get_or_insert_with(move |k| {
                Box::new(KeyExprTreeNode {
                    parent: Some(parent),
                    chunk: k.into(),
                    children: Default::default(),
                    weight: None,
                })
            })
        }
        node
    }
    type TreeIterItemMut = <Self::TreeIterMut as Iterator>::Item;
    type TreeIterMut =
        TreeIterMut<'a, Children, Box<KeyExprTreeNode<Weight, Wildness, Children>>, Weight>;
    fn tree_iter_mut(&'a mut self) -> Self::TreeIterMut {
        TreeIterMut::new(&mut self.children)
    }

    type IntersectionItemMut = <Self::IntersectionMut as Iterator>::Item;
    type IntersectionMut = IterOrOption<
        IntersectionMut<'a, Children, Box<KeyExprTreeNode<Weight, Wildness, Children>>, Weight>,
        &'a mut Self::Node,
    >;
    fn intersecting_nodes_mut(&'a mut self, ke: &'a keyexpr) -> Self::IntersectionMut {
        if self.wildness.get() || ke.is_wild() {
            IntersectionMut::new(&mut self.children, ke).into()
        } else {
            let node = self.node_mut(ke);
            IterOrOption::Opt(node)
        }
    }
    type InclusionItemMut = <Self::InclusionMut as Iterator>::Item;
    type InclusionMut = IterOrOption<
        InclusionMut<'a, Children, Box<KeyExprTreeNode<Weight, Wildness, Children>>, Weight>,
        &'a mut Self::Node,
    >;
    fn included_nodes_mut(&'a mut self, ke: &'a keyexpr) -> Self::InclusionMut {
        if self.wildness.get() || ke.is_wild() {
            InclusionMut::new(&mut self.children, ke).into()
        } else {
            let node = self.node_mut(ke);
            IterOrOption::Opt(node)
        }
    }

    fn prune_where<F: FnMut(&mut Self::Node) -> bool>(&mut self, mut predicate: F) {
        let mut wild = false;
        self.children
            .filter_out(&mut |child| match child.as_mut()._prune(&mut predicate) {
                PruneResult::Delete => true,
                PruneResult::NonWild => false,
                PruneResult::Wild => {
                    wild = true;
                    false
                }
            });
        self.wildness.set(wild);
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
    _item: std::marker::PhantomData<Item>,
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

#[repr(C)]
pub struct KeyExprTreeNode<Weight, Wildness: IWildness, Children: IChildrenProvider<Box<Self>>> {
    parent: Option<NonNull<Self>>,
    chunk: OwnedKeyExpr,
    children: Children::Assoc,
    weight: Option<Weight>,
}

impl<Weight, Wildness: IWildness, Children: IChildrenProvider<Box<Self>>> IKeyExprTreeNode<Weight>
    for KeyExprTreeNode<Weight, Wildness, Children>
where
    Children::Assoc: IChildren<Box<Self>>,
{
    type Parent = Self;
    fn parent(&self) -> Option<&Self> {
        self.parent.as_ref().map(|node| unsafe {
            // this is safe, as a mutable reference to the parent was needed to get a mutable reference to this node in the first place.
            node.as_ref()
        })
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
    type Child = Box<Self>;
    type Children = Children::Assoc;

    fn children(&self) -> &Self::Children {
        &self.children
    }
}
impl<Weight, Wildness: IWildness, Children: IChildrenProvider<Box<Self>>>
    IKeyExprTreeNodeMut<Weight> for KeyExprTreeNode<Weight, Wildness, Children>
where
    Children::Assoc: IChildren<Box<Self>>,
{
    fn parent_mut(&mut self) -> Option<&mut Self> {
        match &mut self.parent {
            None => None,
            Some(node) => Some(unsafe {
                // this is safe, as a mutable reference to the parent was needed to get a mutable reference to this node in the first place.
                node.as_mut()
            }),
        }
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

    fn children_mut(&mut self) -> &mut Self::Children {
        &mut self.children
    }
}

impl<Weight, Wildness: IWildness, Children: IChildrenProvider<Box<Self>>>
    KeyExprTreeNode<Weight, Wildness, Children>
where
    Children::Assoc: IChildren<Box<Self>>,
{
    fn _keyexpr(&self, capacity: usize) -> String {
        let mut s = match self.parent() {
            Some(parent) => parent._keyexpr(capacity + self.chunk.len() + 1) + "/",
            None => String::with_capacity(capacity + self.chunk.len()),
        };
        s.push_str(self.chunk.as_str());
        s
    }
    fn _prune<F: FnMut(&mut Self) -> bool>(&mut self, predicate: &mut F) -> PruneResult {
        let mut result = PruneResult::NonWild;
        self.children
            .filter_out(&mut |child| match child.as_node_mut()._prune(predicate) {
                PruneResult::Delete => true,
                PruneResult::NonWild => false,
                PruneResult::Wild => {
                    result = PruneResult::Wild;
                    false
                }
            });
        if predicate(self) && self.children.is_empty() {
            result = PruneResult::Delete
        } else if self.chunk.is_wild() {
            result = PruneResult::Wild
        }
        result
    }
}
enum PruneResult {
    Delete,
    NonWild,
    Wild,
}

impl<Weight, Wildness: IWildness, Children: IChildrenProvider<Box<Self>>> HasChunk
    for KeyExprTreeNode<Weight, Wildness, Children>
{
    fn chunk(&self) -> &keyexpr {
        &self.chunk
    }
}
impl<Weight, Wildness: IWildness, Children: IChildrenProvider<Box<Self>>> AsRef<Self>
    for KeyExprTreeNode<Weight, Wildness, Children>
{
    fn as_ref(&self) -> &Self {
        self
    }
}
impl<Weight, Wildness: IWildness, Children: IChildrenProvider<Box<Self>>> AsMut<Self>
    for KeyExprTreeNode<Weight, Wildness, Children>
{
    fn as_mut(&mut self) -> &mut Self {
        self
    }
}

// pub struct KeTreePair<Weight: 'static> {
//     non_wilds: KeBoxTree<Weight, NonWild, KeyedSetProvider>,
//     wilds: KeBoxTree<Weight, UnknownWildness, KeyedSetProvider>,
// }

// impl<Weight: 'static> Default for KeTreePair<Weight> {
//     fn default() -> Self {
//         Self {
//             non_wilds: Default::default(),
//             wilds: Default::default(),
//         }
//     }
// }

// impl<Weight: 'static> KeTreePair<Weight> {
//     pub fn new() -> Self {
//         Self::default()
//     }
// }
// impl<'a, Weight: 'a> IKeyExprTree<'a, Weight> for KeTreePair<Weight> {
//     type Node = &'a KeyExprTreeNode<Weight, UnknownWildness, KeyedSetProvider>;

//     fn node(&'a self, at: &keyexpr) -> Option<Self::Node> {
//         if at.is_wild() {
//             self.wilds.node(at)
//         } else {
//             unsafe { std::mem::transmute(self.non_wilds.node(at)) }
//         }
//     }

//     type TreeIterItem = Self::Node;
//     type TreeIter = TransmuteChain<
//         Coerced<<KeBoxTree<Weight, NonWild, KeyedSetProvider> as IKeyExprTree<'a, Weight>>::TreeIter, <KeBoxTree<Weight, NonWild, KeyedSetProvider> as IKeyExprTree<'a, Weight>>::Node>,
//         Coerced<<KeBoxTree<Weight, UnknownWildness, KeyedSetProvider> as IKeyExprTree<'a, Weight>>::TreeIter, Self::Node>,
//     >;

//     fn tree_iter(&'a self) -> Self::TreeIter {
//         Coerced::new(self.non_wilds.tree_iter()).tchain(Coerced::new(self.wilds.tree_iter()))
//     }

//     type IntersectionItem = &'a Self::Node;
//     type Intersection = TransmuteChain<
//         <KeBoxTree<Weight, NonWild, KeyedSetProvider> as IKeyExprTree<'a, Weight>>::Intersection,
//         <KeBoxTree<Weight, UnknownWildness, KeyedSetProvider> as IKeyExprTree<'a, Weight>>::Intersection,
//     >;

//     fn intersecting_nodes(&'a self, key: &'a keyexpr) -> Self::Intersection {
//         self.non_wilds
//             .intersecting_nodes(key)
//             .tchain(self.wilds.intersecting_nodes(key))
//     }
//     type InclusionItem = &'a Self::Node;
//     type Inclusion = TransmuteChain<
//         <KeBoxTree<Weight, NonWild, KeyedSetProvider> as IKeyExprTree<'a, Weight>>::Inclusion,
//         <KeBoxTree<Weight, UnknownWildness, KeyedSetProvider> as IKeyExprTree<'a, Weight>>::Inclusion
//     >;

//     fn included_nodes(&'a self, key: &'a keyexpr) -> Self::Inclusion {
//         self.non_wilds
//             .included_nodes(key)
//             .tchain(self.wilds.included_nodes(key))
//     }
// }
// impl<'a, Weight: 'a> IKeyExprTreeMut<'a, Weight> for KeTreePair<Weight> {
//     type NodeMut = KeyExprTreeNode<Weight, UnknownWildness, KeyedSetProvider>;

//     fn node_mut(&mut self, at: &keyexpr) -> Option<&mut Self::Node> {
//         if at.is_wild() {
//             self.wilds.node_mut(at)
//         } else {
//             unsafe { std::mem::transmute(self.non_wilds.node_mut(at)) }
//         }
//     }

//     fn remove(&mut self, at: &keyexpr) -> Option<Weight> {
//         if at.is_wild() {
//             self.wilds.remove(at)
//         } else {
//             self.non_wilds.remove(at)
//         }
//     }

//     fn node_mut_or_create(&mut self, at: &keyexpr) -> &mut Self::Node {
//         if at.is_wild() {
//             self.wilds.node_mut_or_create(at)
//         } else {
//             unsafe { std::mem::transmute(self.non_wilds.node_mut_or_create(at)) }
//         }
//     }

//     type TreeIterItemMut = &'a mut Self::Node;
//     type TreeIterMut = TransmuteChain<
//     Coerced<<KeBoxTree<Weight, NonWild, KeyedSetProvider> as IKeyExprTreeMut<'a, Weight>>::TreeIterMut, &'a mut <KeBoxTree<Weight, NonWild, KeyedSetProvider> as IKeyExprTree<'a, Weight>>::Node>,
//     Coerced<<KeBoxTree<Weight, UnknownWildness, KeyedSetProvider> as IKeyExprTreeMut<'a,Weight>>::TreeIterMut, &'a mut Self::Node>,
// >;

//     fn tree_iter_mut(&'a mut self) -> Self::TreeIterMut {
//         Coerced::new(self.non_wilds.tree_iter_mut())
//             .tchain(Coerced::new(self.wilds.tree_iter_mut()))
//     }

//     type IntersectionItemMut = &'a mut Self::Node;
//     type IntersectionMut = TransmuteChain<
//     <KeBoxTree<Weight, NonWild, KeyedSetProvider> as IKeyExprTreeMut<'a, Weight>>::IntersectionMut,
//     <KeBoxTree<Weight, UnknownWildness, KeyedSetProvider> as IKeyExprTreeMut<'a, Weight>>::IntersectionMut,
// >;

//     fn intersecting_nodes_mut(&'a mut self, key: &'a keyexpr) -> Self::IntersectionMut {
//         self.non_wilds
//             .intersecting_nodes_mut(key)
//             .tchain(self.wilds.intersecting_nodes_mut(key))
//     }

//     type InclusionItemMut = &'a mut Self::Node;
//     type InclusionMut = TransmuteChain<
//             <KeBoxTree<Weight, NonWild, KeyedSetProvider> as IKeyExprTreeMut<'a, Weight>>::InclusionMut,
//             <KeBoxTree<Weight, UnknownWildness, KeyedSetProvider> as IKeyExprTreeMut<'a, Weight>>::InclusionMut
//     >;

//     fn included_nodes_mut(&'a mut self, key: &'a keyexpr) -> Self::InclusionMut {
//         self.non_wilds
//             .included_nodes_mut(key)
//             .tchain(self.wilds.included_nodes_mut(key))
//     }

//     fn prune_where<F: FnMut(&mut Self::Node) -> bool>(&mut self, mut predicate: F) {
//         self.non_wilds
//             .prune_where(|node| predicate(unsafe { std::mem::transmute(node) }));
//         self.wilds.prune_where(predicate)
//     }
// }

// enum TransmuteChainInner<A, B> {
//     First(A, B),
//     Second(B),
//     Done,
// }
// pub struct TransmuteChain<A, B> {
//     inner: TransmuteChainInner<A, B>,
// }
// trait ITChain<T: Sized>: Sized {
//     fn tchain(self, chain: T) -> TransmuteChain<Self, T>;
// }
// impl<T: Iterator + Sized, U: Iterator + Sized> ITChain<U> for T {
//     fn tchain(self, chain: U) -> TransmuteChain<Self, U> {
//         TransmuteChain {
//             inner: TransmuteChainInner::First(self, chain),
//         }
//     }
// }
// impl<A: Iterator, B: Iterator> Iterator for TransmuteChain<A, B>
// where
//     A::Item: TransmuteInto<B::Item>,
// {
//     type Item = B::Item;

//     fn next(&mut self) -> Option<Self::Item> {
//         match &mut self.inner {
//             TransmuteChainInner::First(a, _) => match a.next() {
//                 Some(i) => Some(i.transmute_into()),
//                 None => unsafe {
//                     let TransmuteChainInner::First(_a, b) = std::ptr::read(&self.inner) else {std::hint::unreachable_unchecked()};
//                     std::ptr::write(&mut self.inner, TransmuteChainInner::Second(b));
//                     self.next()
//                 },
//             },
//             TransmuteChainInner::Second(b) => match b.next() {
//                 Some(i) => Some(i),
//                 None => {
//                     self.inner = TransmuteChainInner::Done;
//                     None
//                 }
//             },
//             TransmuteChainInner::Done => None,
//         }
//     }
// }

trait TransmuteInto<T> {
    fn transmute_into(self) -> T;
}
impl<'a, Weight: 'static>
    TransmuteInto<&'a mut KeyExprTreeNode<Weight, UnknownWildness, KeyedSetProvider>>
    for &'a mut KeyExprTreeNode<Weight, NonWild, KeyedSetProvider>
{
    fn transmute_into(self) -> &'a mut KeyExprTreeNode<Weight, UnknownWildness, KeyedSetProvider> {
        unsafe { std::mem::transmute(self) }
    }
}
impl<'a, Weight: 'static>
    TransmuteInto<&'a KeyExprTreeNode<Weight, UnknownWildness, KeyedSetProvider>>
    for &'a KeyExprTreeNode<Weight, NonWild, KeyedSetProvider>
{
    fn transmute_into(self) -> &'a KeyExprTreeNode<Weight, UnknownWildness, KeyedSetProvider> {
        unsafe { std::mem::transmute(self) }
    }
}

impl<
        'a,
        K: AsRef<keyexpr>,
        Weight,
        Wildness: IWildness,
        Children: IChildrenProvider<Box<KeyExprTreeNode<Weight, Wildness, Children>>>,
    > std::iter::FromIterator<(K, Weight)> for KeBoxTree<Weight, Wildness, Children>
where
    Self: IKeyExprTreeMut<'a, Weight>,
{
    fn from_iter<T: IntoIterator<Item = (K, Weight)>>(iter: T) -> Self {
        let mut tree = Self::default();
        for (key, value) in iter {
            tree.insert(key.as_ref(), value);
        }
        tree
    }
}

// impl<K: AsRef<keyexpr>, Weight> std::iter::FromIterator<(K, Weight)> for KeTreePair<Weight> {
//     fn from_iter<T: IntoIterator<Item = (K, Weight)>>(iter: T) -> Self {
//         let mut tree = Self::default();
//         for (key, value) in iter {
//             tree.node_mut_or_create(key.as_ref()).weight = Some(value);
//         }
//         tree
//     }
// }
