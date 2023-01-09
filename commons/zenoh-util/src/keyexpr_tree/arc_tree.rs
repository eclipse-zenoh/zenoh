use std::sync::{Arc, Weak};

use token_cell::prelude::*;

use crate::keyexpr_tree::*;
use zenoh_protocol::core::key_expr::keyexpr;

pub struct KeArcTree<
    Weight,
    Wildness: IWildness,
    Children: IChildrenProvider<Arc<TokenCell<KeArcTreeNode<Weight, Wildness, Children, Token>, Token>>>,
    Token: TokenTrait,
> {
    children: Children::Assoc,
    wildness: Wildness,
}

pub struct KeArcTreeNode<
    Weight,
    Wildness: IWildness,
    Children: IChildrenProvider<Arc<TokenCell<Self, Token>>>,
    Token: TokenTrait,
> {
    parent: Option<Weak<TokenCell<Self, Token>>>,
    chunk: OwnedKeyExpr,
    children: Children::Assoc,
    weight: Option<Weight>,
}

impl<
        Weight,
        Wildness: IWildness,
        Children: IChildrenProvider<Arc<TokenCell<Self, Token>>>,
        Token: TokenTrait,
    > IKeyExprTreeNode<Weight> for KeArcTreeNode<Weight, Wildness, Children, Token>
where
    Children::Assoc: IChildren<Arc<TokenCell<Self, Token>>>,
{
    type Parent = Weak<TokenCell<Self, Token>>;
    fn parent(&self) -> Option<&Self::Parent> {
        self.parent.as_ref()
    }
    fn parent_mut(&mut self) -> Option<&mut Self::Parent> {
        self.parent.as_mut()
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
    fn weight_mut(&mut self) -> Option<&mut Weight> {
        self.weight.as_mut()
    }
    fn take_weight(&mut self) -> Option<Weight> {
        self.weight.take()
    }
    fn insert_weight(&mut self, weight: Weight) -> Option<Weight> {
        self.weight.replace(weight)
    }

    type Child = Arc<TokenCell<Self, Token>>;
    type Children = Children::Assoc;
    fn children(&self) -> &Self::Children {
        &self.children
    }
    fn children_mut(&mut self) -> &mut Self::Children {
        &mut self.children
    }
}

impl<
        Weight,
        Wildness: IWildness,
        Children: IChildrenProvider<Arc<TokenCell<Self, Token>>>,
        Token: TokenTrait,
    > KeArcTreeNode<Weight, Wildness, Children, Token>
where
    Children::Assoc: IChildren<Arc<TokenCell<Self, Token>>>,
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
