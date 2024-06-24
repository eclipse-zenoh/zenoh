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

use alloc::{
    string::String,
    sync::{Arc, Weak},
};
use core::fmt::Debug;

use token_cell::prelude::*;

use super::{box_tree::PruneResult, support::IterOrOption};
use crate::{
    keyexpr,
    keyexpr_tree::{support::IWildness, *},
};

pub struct KeArcTreeInner<
    Weight,
    Wildness: IWildness,
    Children: IChildrenProvider<
        Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    >,
    Token: TokenTrait,
> {
    children: Children::Assoc,
    wildness: Wildness,
}

token_cell::token!(pub DefaultToken);
fn ketree_borrow<'a, T, Token: TokenTrait>(
    cell: &'a TokenCell<T, Token>,
    token: &'a Token,
) -> &'a T {
    cell.try_borrow(token)
        .unwrap_or_else(|_| panic!("Attempted to use KeArcTree with the wrong Token"))
}
fn ketree_borrow_mut<'a, T, Token: TokenTrait>(
    cell: &'a TokenCell<T, Token>,
    token: &'a mut Token,
) -> &'a mut T {
    cell.try_borrow_mut(token)
        .unwrap_or_else(|_| panic!("Attempted to mutably use KeArcTree with the wrong Token"))
}

/// A shared KeTree.
///
/// The tree and its nodes have shared ownership, while their mutability is managed through the `Token`.
///
/// Most of its methods are declared in the [`ITokenKeyExprTree`] trait.
// tags{ketree.arc}
pub struct KeArcTree<
    Weight,
    Token: TokenTrait = DefaultToken,
    Wildness: IWildness = bool,
    Children: IChildrenProvider<
        Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    > = DefaultChildrenProvider,
> {
    inner: TokenCell<KeArcTreeInner<Weight, Wildness, Children, Token>, Token>,
}

impl<
        Weight,
        Wildness: IWildness,
        Children: IChildrenProvider<
            Arc<
                TokenCell<
                    KeArcTreeNode<Weight, Weak<()>, Wildness, Children, DefaultToken>,
                    DefaultToken,
                >,
            >,
        >,
    > KeArcTree<Weight, DefaultToken, Wildness, Children>
{
    /// Constructs the KeArcTree, returning it and its token, unless constructing the Token failed.
    ///
    /// # Type inference papercut
    /// Despite some of `KeArcTree`'s generic parameters having default values, those are only taken into
    /// account by the compiler when a type is named with some parameters omitted, and not when a type is
    /// inferred with the same parameters unconstrained.
    ///
    /// The simplest way to resolve this is to eventually assign to tree part of the return value
    /// to a variable or field whose type is named `KeArcTree<_>` (the `Weight` parameter can generally be inferred).
    pub fn new() -> Result<(Self, DefaultToken), <DefaultToken as TokenTrait>::ConstructionError> {
        let token = DefaultToken::new()?;
        Ok((Self::with_token(&token), token))
    }
}

impl<
        Weight,
        Wildness: IWildness,
        Children: IChildrenProvider<
            Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
        >,
        Token: TokenTrait,
    > KeArcTree<Weight, Token, Wildness, Children>
{
    /// Constructs the KeArcTree with a specific token.
    pub fn with_token(token: &Token) -> Self {
        Self {
            inner: TokenCell::new(
                KeArcTreeInner {
                    children: Default::default(),
                    wildness: Wildness::non_wild(),
                },
                token,
            ),
        }
    }
}

#[allow(clippy::type_complexity)]
impl<
        'a,
        Weight: 'a,
        Wildness: IWildness + 'a,
        Children: IChildrenProvider<
                Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
            > + 'a,
        Token: TokenTrait + 'a,
    > ITokenKeyExprTree<'a, Weight, Token> for KeArcTree<Weight, Token, Wildness, Children>
where
    Children::Assoc: IChildren<
        Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    >,
{
    type Node = (
        &'a Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
        &'a Token,
    );
    type NodeMut = (
        &'a Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
        &'a mut Token,
    );
    // tags{ketree.arc.node}
    fn node(&'a self, token: &'a Token, at: &keyexpr) -> Option<Self::Node> {
        let inner = ketree_borrow(&self.inner, token);
        let mut chunks = at.chunks_impl();
        let mut node = inner.children.child_at(chunks.next().unwrap())?;
        for chunk in chunks {
            let as_node: &Arc<
                TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>,
            > = node.as_node();
            node = unsafe { (*as_node.get()).children.child_at(chunk)? };
        }
        Some((node.as_node(), token))
    }
    // tags{ketree.arc.node.mut}
    fn node_mut(&'a self, token: &'a mut Token, at: &keyexpr) -> Option<Self::NodeMut> {
        self.node(
            unsafe { core::mem::transmute::<&Token, &Token>(&*token) },
            at,
        )
        .map(|(node, _)| (node, token))
    }
    // tags{ketree.arc.node.or_create}
    fn node_or_create(&'a self, token: &'a mut Token, at: &keyexpr) -> Self::NodeMut {
        let inner = ketree_borrow_mut(&self.inner, token);
        if at.is_wild_impl() {
            inner.wildness.set(true);
        }
        let inner: &mut KeArcTreeInner<Weight, Wildness, Children, Token> =
            unsafe { core::mem::transmute(inner) };
        let construct_node = |k: &keyexpr, parent| {
            Arc::new(TokenCell::new(
                KeArcTreeNode {
                    parent,
                    chunk: k.into(),
                    children: Default::default(),
                    weight: None,
                },
                token,
            ))
        };
        let mut chunks = at.chunks_impl();
        let mut node = inner
            .children
            .entry(chunks.next().unwrap())
            .get_or_insert_with(|k| construct_node(k, None));
        for chunk in chunks {
            let as_node: &Arc<
                TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>,
            > = node.as_node();
            node = unsafe {
                (*as_node.get())
                    .children
                    .entry(chunk)
                    .get_or_insert_with(|k| construct_node(k, Some(Arc::downgrade(as_node))))
            };
        }
        (node, token)
    }

    type TreeIterItem = Self::Node;
    type TreeIter = TokenPacker<
        TreeIter<
            'a,
            Children,
            Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
            Weight,
        >,
        &'a Token,
    >;
    // tags{ketree.arc.tree_iter}
    fn tree_iter(&'a self, token: &'a Token) -> Self::TreeIter {
        let inner = ketree_borrow(&self.inner, token);
        TokenPacker {
            iter: TreeIter::new(&inner.children),
            token,
        }
    }

    type TreeIterItemMut = Tokenized<
        &'a Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
        &'a mut Token,
    >;
    type TreeIterMut = TokenPacker<
        TreeIter<
            'a,
            Children,
            Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
            Weight,
        >,
        &'a mut Token,
    >;
    // tags{ketree.arc.tree_iter.mut}
    fn tree_iter_mut(&'a self, token: &'a mut Token) -> Self::TreeIterMut {
        let inner = ketree_borrow(&self.inner, token);
        TokenPacker {
            iter: TreeIter::new(unsafe {
                core::mem::transmute::<&Children::Assoc, &Children::Assoc>(&inner.children)
            }),
            token,
        }
    }

    type IntersectionItem = Self::Node;
    type Intersection = IterOrOption<
        TokenPacker<
            Intersection<
                'a,
                Children,
                Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
                Weight,
            >,
            &'a Token,
        >,
        Self::IntersectionItem,
    >;
    // tags{ketree.arc.intersecting}
    fn intersecting_nodes(&'a self, token: &'a Token, key: &'a keyexpr) -> Self::Intersection {
        let inner = ketree_borrow(&self.inner, token);
        if inner.wildness.get() || key.is_wild_impl() {
            IterOrOption::Iter(TokenPacker {
                iter: Intersection::new(&inner.children, key),
                token,
            })
        } else {
            IterOrOption::Opt(self.node(token, key))
        }
    }
    type IntersectionItemMut = Self::TreeIterItemMut;
    type IntersectionMut = IterOrOption<
        TokenPacker<
            Intersection<
                'a,
                Children,
                Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
                Weight,
            >,
            &'a mut Token,
        >,
        Self::IntersectionItemMut,
    >;
    // tags{ketree.arc.intersecting.mut}
    fn intersecting_nodes_mut(
        &'a self,
        token: &'a mut Token,
        key: &'a keyexpr,
    ) -> Self::IntersectionMut {
        let inner = ketree_borrow(&self.inner, token);
        if inner.wildness.get() || key.is_wild_impl() {
            IterOrOption::Iter(TokenPacker {
                iter: Intersection::new(
                    unsafe {
                        core::mem::transmute::<&Children::Assoc, &Children::Assoc>(&inner.children)
                    },
                    key,
                ),
                token,
            })
        } else {
            IterOrOption::Opt(self.node_mut(token, key).map(Into::into))
        }
    }

    type InclusionItem = Self::Node;
    type Inclusion = IterOrOption<
        TokenPacker<
            Inclusion<
                'a,
                Children,
                Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
                Weight,
            >,
            &'a Token,
        >,
        Self::InclusionItem,
    >;
    // tags{ketree.arc.included}
    fn included_nodes(&'a self, token: &'a Token, key: &'a keyexpr) -> Self::Inclusion {
        let inner = ketree_borrow(&self.inner, token);
        if inner.wildness.get() || key.is_wild_impl() {
            IterOrOption::Iter(TokenPacker {
                iter: Inclusion::new(&inner.children, key),
                token,
            })
        } else {
            IterOrOption::Opt(self.node(token, key))
        }
    }
    type InclusionItemMut = Self::TreeIterItemMut;
    type InclusionMut = IterOrOption<
        TokenPacker<
            Inclusion<
                'a,
                Children,
                Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
                Weight,
            >,
            &'a mut Token,
        >,
        Self::InclusionItemMut,
    >;
    // tags{ketree.arc.included.mut}
    fn included_nodes_mut(&'a self, token: &'a mut Token, key: &'a keyexpr) -> Self::InclusionMut {
        let inner = ketree_borrow(&self.inner, token);
        if inner.wildness.get() || key.is_wild_impl() {
            unsafe {
                IterOrOption::Iter(TokenPacker {
                    iter: Inclusion::new(
                        core::mem::transmute::<&Children::Assoc, &Children::Assoc>(&inner.children),
                        key,
                    ),
                    token,
                })
            }
        } else {
            IterOrOption::Opt(self.node_mut(token, key).map(Into::into))
        }
    }

    type IncluderItem = Self::Node;
    type Includer = IterOrOption<
        TokenPacker<
            Includer<
                'a,
                Children,
                Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
                Weight,
            >,
            &'a Token,
        >,
        Self::IncluderItem,
    >;
    // tags{ketree.arc.including}
    fn nodes_including(&'a self, token: &'a Token, key: &'a keyexpr) -> Self::Includer {
        let inner = ketree_borrow(&self.inner, token);
        if inner.wildness.get() || key.is_wild_impl() {
            IterOrOption::Iter(TokenPacker {
                iter: Includer::new(&inner.children, key),
                token,
            })
        } else {
            IterOrOption::Opt(self.node(token, key))
        }
    }
    type IncluderItemMut = Self::TreeIterItemMut;
    type IncluderMut = IterOrOption<
        TokenPacker<
            Includer<
                'a,
                Children,
                Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
                Weight,
            >,
            &'a mut Token,
        >,
        Self::IncluderItemMut,
    >;
    // tags{ketree.arc.including.mut}
    fn nodes_including_mut(&'a self, token: &'a mut Token, key: &'a keyexpr) -> Self::IncluderMut {
        let inner = ketree_borrow(&self.inner, token);
        if inner.wildness.get() || key.is_wild_impl() {
            unsafe {
                IterOrOption::Iter(TokenPacker {
                    iter: Includer::new(
                        core::mem::transmute::<&Children::Assoc, &Children::Assoc>(&inner.children),
                        key,
                    ),
                    token,
                })
            }
        } else {
            IterOrOption::Opt(self.node_mut(token, key).map(Into::into))
        }
    }
    type PruneNode = KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>;

    // tags{ketree.arc.prune.where}
    fn prune_where<F: FnMut(&mut Self::PruneNode) -> bool>(
        &self,
        token: &mut Token,
        mut predicate: F,
    ) {
        let mut wild = false;
        let inner = ketree_borrow_mut(&self.inner, token);
        inner.children.filter_out(
            &mut |child| match unsafe { (*child.get()).prune(&mut predicate) } {
                PruneResult::Delete => Arc::strong_count(child) <= 1,
                PruneResult::NonWild => false,
                PruneResult::Wild => {
                    wild = true;
                    false
                }
            },
        );
        inner.wildness.set(wild);
    }
}

pub(crate) mod sealed {
    use alloc::sync::Arc;
    use core::ops::{Deref, DerefMut};

    use token_cell::prelude::{TokenCell, TokenTrait};

    pub struct Tokenized<A, B>(pub A, pub(crate) B);
    impl<T, Token: TokenTrait> Deref for Tokenized<&TokenCell<T, Token>, &Token> {
        type Target = T;
        fn deref(&self) -> &Self::Target {
            unsafe { &*self.0.get() }
        }
    }
    impl<T, Token: TokenTrait> Deref for Tokenized<&TokenCell<T, Token>, &mut Token> {
        type Target = T;
        fn deref(&self) -> &Self::Target {
            unsafe { &*self.0.get() }
        }
    }
    impl<T, Token: TokenTrait> DerefMut for Tokenized<&TokenCell<T, Token>, &mut Token> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            unsafe { &mut *self.0.get() }
        }
    }
    impl<T, Token: TokenTrait> Deref for Tokenized<&Arc<TokenCell<T, Token>>, &Token> {
        type Target = T;
        fn deref(&self) -> &Self::Target {
            unsafe { &*self.0.get() }
        }
    }
    impl<T, Token: TokenTrait> Deref for Tokenized<&Arc<TokenCell<T, Token>>, &mut Token> {
        type Target = T;
        fn deref(&self) -> &Self::Target {
            unsafe { &*self.0.get() }
        }
    }
    impl<T, Token: TokenTrait> DerefMut for Tokenized<&Arc<TokenCell<T, Token>>, &mut Token> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            unsafe { &mut *self.0.get() }
        }
    }
    impl<A, B> From<(A, B)> for Tokenized<A, B> {
        fn from((a, b): (A, B)) -> Self {
            Self(a, b)
        }
    }
    pub struct TokenPacker<I, T> {
        pub(crate) iter: I,
        pub(crate) token: T,
    }

    impl<'a, I: Iterator, T> Iterator for TokenPacker<I, &'a T> {
        type Item = (I::Item, &'a T);
        fn next(&mut self) -> Option<Self::Item> {
            self.iter.next().map(|i| (i, self.token))
        }
    }

    impl<'a, I: Iterator, T> Iterator for TokenPacker<I, &'a mut T> {
        type Item = Tokenized<I::Item, &'a mut T>;
        fn next(&mut self) -> Option<Self::Item> {
            self.iter.next().map(|i| {
                Tokenized(i, unsafe {
                    // SAFETY: while this makes it possible for multiple mutable references to the Token to exist,
                    // it prevents them from being extracted and thus used to create multiple mutable references to
                    // a same memory address.
                    core::mem::transmute_copy(&self.token)
                })
            })
        }
    }
}
pub use sealed::{TokenPacker, Tokenized};

pub trait IArcProvider {
    type Ptr<T>: IArc<T>;
}
pub trait IArc<T> {
    fn weak(&self) -> Weak<T>;
    type UpgradeErr: Debug;
    fn upgrade(&self) -> Result<Arc<T>, Self::UpgradeErr>;
}
impl IArcProvider for Arc<()> {
    type Ptr<T> = Arc<T>;
}
impl IArcProvider for Weak<()> {
    type Ptr<T> = Weak<T>;
}
impl<T> IArc<T> for Arc<T> {
    fn weak(&self) -> Weak<T> {
        Arc::downgrade(self)
    }
    type UpgradeErr = core::convert::Infallible;
    fn upgrade(&self) -> Result<Arc<T>, core::convert::Infallible> {
        Ok(self.clone())
    }
}
#[derive(Debug, Clone, Copy)]
pub struct WeakConvertError;
impl<T> IArc<T> for Weak<T> {
    fn weak(&self) -> Weak<T> {
        self.clone()
    }
    type UpgradeErr = WeakConvertError;
    fn upgrade(&self) -> Result<Arc<T>, WeakConvertError> {
        Weak::upgrade(self).ok_or(WeakConvertError)
    }
}

#[allow(clippy::type_complexity)]
pub struct KeArcTreeNode<
    Weight,
    Parent: IArcProvider,
    Wildness: IWildness,
    Children: IChildrenProvider<
        Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    >,
    Token: TokenTrait,
> {
    parent: Option<
        Parent::Ptr<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    >,
    chunk: OwnedKeyExpr,
    children: Children::Assoc,
    weight: Option<Weight>,
}

impl<
        Weight,
        Wildness: IWildness,
        Children: IChildrenProvider<
            Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
        >,
        Token: TokenTrait,
    > KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>
where
    Children::Assoc: IChildren<
        Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    >,
{
    fn prune<F: FnMut(&mut Self) -> bool>(&mut self, predicate: &mut F) -> PruneResult {
        let mut result = PruneResult::NonWild;
        self.children.filter_out(&mut |child| {
            let c = unsafe { &mut *child.get() };
            match c.prune(predicate) {
                PruneResult::Delete => Arc::strong_count(child) <= 1,
                PruneResult::NonWild => false,
                PruneResult::Wild => {
                    result = PruneResult::Wild;
                    false
                }
            }
        });
        if predicate(self) && self.children.is_empty() {
            result = PruneResult::Delete
        } else if self.chunk.is_wild_impl() {
            result = PruneResult::Wild
        }
        result
    }
}

impl<
        Weight,
        Parent: IArcProvider,
        Wildness: IWildness,
        Children: IChildrenProvider<
            Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
        >,
        Token: TokenTrait,
    > UIKeyExprTreeNode<Weight>
    for Arc<TokenCell<KeArcTreeNode<Weight, Parent, Wildness, Children, Token>, Token>>
where
    Children::Assoc: IChildren<
        Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    >,
{
    type Parent = <KeArcTreeNode<Weight, Parent, Wildness, Children, Token> as UIKeyExprTreeNode<
        Weight,
    >>::Parent;
    unsafe fn __parent(&self) -> Option<&Self::Parent> {
        (*self.get()).parent()
    }

    unsafe fn __keyexpr(&self) -> OwnedKeyExpr {
        (*self.get()).keyexpr()
    }

    unsafe fn __weight(&self) -> Option<&Weight> {
        (*self.get()).weight()
    }

    type Child = <KeArcTreeNode<Weight, Parent, Wildness, Children, Token> as UIKeyExprTreeNode<
        Weight,
    >>::Child;

    type Children= <KeArcTreeNode<Weight, Parent, Wildness, Children, Token> as UIKeyExprTreeNode<
    Weight,
>>::Children;

    unsafe fn __children(&self) -> &Self::Children {
        (*self.get()).children()
    }
}

impl<
        Weight,
        Parent: IArcProvider,
        Wildness: IWildness,
        Children: IChildrenProvider<
            Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
        >,
        Token: TokenTrait,
    > HasChunk for KeArcTreeNode<Weight, Parent, Wildness, Children, Token>
{
    fn chunk(&self) -> &keyexpr {
        &self.chunk
    }
}

impl<
        Weight,
        Parent: IArcProvider,
        Wildness: IWildness,
        Children: IChildrenProvider<
            Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
        >,
        Token: TokenTrait,
    > IKeyExprTreeNode<Weight> for KeArcTreeNode<Weight, Parent, Wildness, Children, Token>
where
    Children::Assoc: IChildren<
        Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    >,
{
}

impl<
        Weight,
        Parent: IArcProvider,
        Wildness: IWildness,
        Children: IChildrenProvider<
            Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
        >,
        Token: TokenTrait,
    > UIKeyExprTreeNode<Weight> for KeArcTreeNode<Weight, Parent, Wildness, Children, Token>
where
    Children::Assoc: IChildren<
        Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    >,
{
    type Parent =
        Parent::Ptr<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>;
    unsafe fn __parent(&self) -> Option<&Self::Parent> {
        self.parent.as_ref()
    }
    /// May panic if the node has been zombified (see [`Self::is_zombie`])
    unsafe fn __keyexpr(&self) -> OwnedKeyExpr {
        unsafe {
            // self._keyexpr is guaranteed to return a valid KE, so no checks are necessary
            OwnedKeyExpr::from_string_unchecked(self._keyexpr(0))
        }
    }
    unsafe fn __weight(&self) -> Option<&Weight> {
        self.weight.as_ref()
    }

    type Child = Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>;
    type Children = Children::Assoc;
    unsafe fn __children(&self) -> &Self::Children {
        &self.children
    }
}

impl<
        Weight,
        Parent: IArcProvider,
        Wildness: IWildness,
        Children: IChildrenProvider<
            Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
        >,
        Token: TokenTrait,
    > IKeyExprTreeNodeMut<Weight> for KeArcTreeNode<Weight, Parent, Wildness, Children, Token>
where
    Children::Assoc: IChildren<
        Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    >,
{
    fn parent_mut(&mut self) -> Option<&mut Self::Parent> {
        self.parent.as_mut()
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

impl<
        Weight,
        Wildness: IWildness,
        Children: IChildrenProvider<
            Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
        >,
        Token: TokenTrait,
    > KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>
where
    Children::Assoc: IChildren<
        Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    >,
{
    /// Under specific circumstances, a node that's been cloned from a tree might have a destroyed parent.
    /// Such a node is "zombified", and becomes unable to perform certain operations such as navigating its parents,
    /// which may be necessary for operations such as [`IKeyExprTreeNode::keyexpr`].
    ///
    /// To become zombified, a node and its parents must both have been pruned from the tree that constructed them, while at least one of the parents has also been dropped everywhere it was aliased through [`Arc`].
    pub fn is_zombie(&self) -> bool {
        match &self.parent {
            Some(parent) => match parent.upgrade() {
                Some(parent) => unsafe { &*parent.get() }.is_zombie(),
                None => true,
            },
            None => false,
        }
    }
}

impl<
        Weight,
        Parent: IArcProvider,
        Wildness: IWildness,
        Children: IChildrenProvider<
            Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
        >,
        Token: TokenTrait,
    > KeArcTreeNode<Weight, Parent, Wildness, Children, Token>
where
    Children::Assoc: IChildren<
        Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    >,
{
    fn _keyexpr(&self, capacity: usize) -> String {
        let mut s = match self.parent() {
            Some(parent) => {
                let parent = unsafe {
                    &*parent
                        .upgrade()
                        .expect("Attempted to use a zombie KeArcTreeNode (see KeArcTreeNode::is_zombie())")
                        .get()
                };
                parent._keyexpr(capacity + self.chunk.len() + 1) + "/"
            }
            None => String::with_capacity(capacity + self.chunk.len()),
        };
        s.push_str(self.chunk.as_str());
        s
    }
}
