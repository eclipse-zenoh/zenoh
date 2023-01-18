use std::{
    fmt::Debug,
    sync::{Arc, Weak},
};

use token_cell::prelude::*;

use crate::keyexpr_tree::*;
use zenoh_protocol::core::key_expr::keyexpr;

use super::box_tree::IterOrOption;

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
    fn node(&'a self, token: &'a Token, at: &keyexpr) -> Option<Self::Node> {
        let Ok(inner) = self.inner.try_borrow(token) else {panic!("Attempted to use KeArcTree with the wrong Token")};
        let mut chunks = at.chunks();
        let mut node = inner.children.child_at(chunks.next().unwrap())?;
        for chunk in chunks {
            let as_node: &Arc<
                TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>,
            > = node.as_node();
            node = unsafe { (&*as_node.get()).children.child_at(chunk)? };
        }
        Some((node.as_node(), token))
    }
    fn node_mut(&'a self, token: &'a mut Token, at: &keyexpr) -> Option<Self::NodeMut> {
        self.node(unsafe { std::mem::transmute(&*token) }, at)
            .map(|(node, _)| (node, token))
    }
    fn node_or_create(&'a self, token: &'a mut Token, at: &keyexpr) -> Self::NodeMut {
        let Ok(inner) = self.inner.try_borrow_mut(token) else {panic!("Attempted to use KeArcTree with the wrong Token")};
        if at.is_wild() {
            inner.wildness.set(true);
        }
        let inner: &mut KeArcTreeInner<Weight, Wildness, Children, Token> =
            unsafe { std::mem::transmute(inner) };
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
        let mut chunks = at.chunks();
        let mut node = inner
            .children
            .entry(chunks.next().unwrap())
            .get_or_insert_with(|k| construct_node(k, None));
        for chunk in chunks {
            let as_node: &Arc<
                TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>,
            > = node.as_node();
            node = unsafe {
                (&mut *as_node.get())
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
    fn tree_iter(&'a self, token: &'a Token) -> Self::TreeIter {
        let Ok(inner) = self.inner.try_borrow(token) else {panic!("Attempted to use KeArcTree with the wrong Token")};
        TokenPacker {
            iter: TreeIter::new(&inner.children),
            token,
        }
    }

    type TreeIterItemMut = Self::NodeMut;
    type TreeIterMut = TokenPacker<
        TreeIter<
            'a,
            Children,
            Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
            Weight,
        >,
        &'a mut Token,
    >;
    fn tree_iter_mut(&'a self, token: &'a mut Token) -> Self::TreeIterMut {
        let Ok(inner) = self.inner.try_borrow(token) else {panic!("Attempted to use KeArcTree with the wrong Token")};
        TokenPacker {
            iter: TreeIter::new(unsafe { std::mem::transmute(&inner.children) }),
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
    fn intersecting_nodes(&'a self, token: &'a Token, key: &'a keyexpr) -> Self::Intersection {
        let Ok(inner) = self.inner.try_borrow(token) else {panic!("Attempted to use KeArcTree with the wrong Token")};
        if inner.wildness.get() {
            IterOrOption::Iter(TokenPacker {
                iter: Intersection::new(&inner.children, key),
                token,
            })
        } else {
            IterOrOption::Opt(self.node(token, key))
        }
    }
    type IntersectionItemMut = Self::NodeMut;
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
    fn intersecting_nodes_mut(
        &'a self,
        token: &'a mut Token,
        key: &'a keyexpr,
    ) -> Self::IntersectionMut {
        let Ok(inner) = self.inner.try_borrow(token) else {panic!("Attempted to use KeArcTree with the wrong Token")};
        if inner.wildness.get() {
            IterOrOption::Iter(TokenPacker {
                iter: Intersection::new(unsafe { std::mem::transmute(&inner.children) }, key),
                token,
            })
        } else {
            IterOrOption::Opt(self.node_mut(token, key))
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
    fn included_nodes(&'a self, token: &'a Token, key: &'a keyexpr) -> Self::Inclusion {
        let Ok(inner) = self.inner.try_borrow(token) else {panic!("Attempted to use KeArcTree with the wrong Token")};
        if inner.wildness.get() {
            IterOrOption::Iter(TokenPacker {
                iter: Inclusion::new(&inner.children, key),
                token,
            })
        } else {
            IterOrOption::Opt(self.node(token, key))
        }
    }
    type InclusionItemMut = Self::NodeMut;
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
    fn included_nodes_mut(&'a self, token: &'a mut Token, key: &'a keyexpr) -> Self::InclusionMut {
        let Ok(inner) = self.inner.try_borrow(token) else {panic!("Attempted to use KeArcTree with the wrong Token")};
        if inner.wildness.get() {
            IterOrOption::Iter(TokenPacker {
                iter: Inclusion::new(unsafe { std::mem::transmute(&inner.children) }, key),
                token,
            })
        } else {
            IterOrOption::Opt(self.node_mut(token, key))
        }
    }
}
pub struct TokenPacker<I, T> {
    iter: I,
    token: T,
}

impl<I: Iterator, T> Iterator for TokenPacker<I, T> {
    type Item = (I::Item, T);
    fn next(&mut self) -> Option<Self::Item> {
        self.iter
            .next()
            .map(|i| (i, unsafe { std::mem::transmute_copy(&self.token) }))
    }
}
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
    type UpgradeErr = std::convert::Infallible;
    fn upgrade(&self) -> Result<Arc<T>, std::convert::Infallible> {
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
