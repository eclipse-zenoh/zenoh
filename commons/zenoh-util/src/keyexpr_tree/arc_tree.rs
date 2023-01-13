use std::{
    fmt::Debug,
    sync::{Arc, Weak},
};

use token_cell::prelude::*;

use crate::keyexpr_tree::*;
use zenoh_protocol::core::key_expr::keyexpr;

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
pub struct KeArcTree<
    Weight,
    Wildness: IWildness,
    Children: IChildrenProvider<
        Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    >,
    Token: TokenTrait,
> {
    inner: TokenCell<KeArcTreeInner<Weight, Wildness, Children, Token>, Token>,
}

impl<
        'a,
        Weight,
        Wildness: IWildness,
        Children: IChildrenProvider<
            Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
        >,
        Token: TokenTrait,
    > ITokenKeyExprTree<'a, Weight, Token> for KeArcTree<Weight, Wildness, Children, Token>
where
    Children::Assoc: IChildren<
        Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    >,
{
    type Node = Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>;

    fn node(&self, token: &Token, at: &keyexpr) -> Option<Self::Node> {
        let Ok(inner) = self.inner.try_borrow(token) else {panic!("Attempted to use KeArcTree with the wrong Token")};
        let mut chunks = at.chunks();
        let mut node = inner.children.child_at(chunks.next().unwrap())?;
        for chunk in chunks {
            let as_node: &Self::Node = node.as_node();
            node = unsafe { (&*as_node.get()).children.child_at(chunk)? };
        }
        Some(node.as_node().clone())
    }
    fn node_or_create(&self, token: &mut Token, at: &keyexpr) -> Self::Node {
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
            let as_node: &mut Self::Node = node.as_node_mut();
            node = unsafe {
                (&mut *as_node.get())
                    .children
                    .entry(chunk)
                    .get_or_insert_with(|k| construct_node(k, Some(Arc::downgrade(as_node))))
            };
        }
        node.clone()
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
    > IKeyExprTreeNode<Weight> for KeArcTreeNode<Weight, Parent, Wildness, Children, Token>
where
    Children::Assoc: IChildren<
        Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>,
    >,
{
    type Parent =
        Parent::Ptr<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>;
    fn parent(&self) -> Option<&Self::Parent> {
        self.parent.as_ref()
    }
    /// May panic if the node has been zombified (see [`Self::is_zombie`])
    fn keyexpr(&self) -> OwnedKeyExpr {
        unsafe {
            // self._keyexpr is guaranteed to return a valid KE, so no checks are necessary
            OwnedKeyExpr::from_string_unchecked(self._keyexpr(0))
        }
    }
    fn weight(&self) -> Option<&Weight> {
        self.weight.as_ref()
    }

    type Child = Arc<TokenCell<KeArcTreeNode<Weight, Weak<()>, Wildness, Children, Token>, Token>>;
    type Children = Children::Assoc;
    fn children(&self) -> &Self::Children {
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
    pub fn upgrade(self) -> Option<KeArcTreeNode<Weight, Arc<()>, Wildness, Children, Token>> {
        let KeArcTreeNode {
            parent,
            chunk,
            children,
            weight,
        } = self;
        match parent {
            Some(parent) => match parent.upgrade() {
                Some(arc) => {
                    let ke_arc_tree_node = KeArcTreeNode {
                        parent: Some(arc),
                        chunk,
                        children,
                        weight,
                    };
                    Some(ke_arc_tree_node)
                }
                None => None,
            },
            None => Some(KeArcTreeNode {
                parent: None,
                chunk,
                children,
                weight,
            }),
        }
    }
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
