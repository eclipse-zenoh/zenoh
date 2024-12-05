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

use alloc::{boxed::Box, sync::Arc};

use token_cell::prelude::{TokenCell, TokenCellTrait, TokenTrait};

use super::*;

impl<T: HasChunk> HasChunk for &T {
    fn chunk(&self) -> &keyexpr {
        T::chunk(self)
    }
}
impl<T: HasChunk> HasChunk for &mut T {
    fn chunk(&self) -> &keyexpr {
        T::chunk(self)
    }
}
impl<T: HasChunk> HasChunk for Box<T> {
    fn chunk(&self) -> &keyexpr {
        T::chunk(self)
    }
}
impl<T: HasChunk> HasChunk for Arc<T> {
    fn chunk(&self) -> &keyexpr {
        T::chunk(self)
    }
}
impl<T: HasChunk, Token: TokenTrait> HasChunk for TokenCell<T, Token> {
    fn chunk(&self) -> &keyexpr {
        T::chunk(unsafe { &*self.get() })
    }
}
impl<T> AsNode<T> for T {
    fn as_node(&self) -> &T {
        self
    }
}
impl<T> AsNode<T> for &T {
    fn as_node(&self) -> &T {
        self
    }
}
impl<T> AsNode<T> for &mut T {
    fn as_node(&self) -> &T {
        self
    }
}
impl<T> AsNodeMut<T> for T {
    fn as_node_mut(&mut self) -> &mut T {
        self
    }
}
impl<T> AsNodeMut<T> for &mut T {
    fn as_node_mut(&mut self) -> &mut T {
        self
    }
}
impl<T: IKeyExprTreeNode<Weight>, Weight> IKeyExprTreeNode<Weight> for &T {}
impl<T: IKeyExprTreeNode<Weight>, Weight> UIKeyExprTreeNode<Weight> for &T {
    type Parent = T::Parent;
    unsafe fn __parent(&self) -> Option<&Self::Parent> {
        T::__parent(self)
    }
    unsafe fn __keyexpr(&self) -> OwnedKeyExpr {
        T::__keyexpr(self)
    }
    unsafe fn __weight(&self) -> Option<&Weight> {
        T::__weight(self)
    }

    type Child = T::Child;
    type Children = T::Children;

    unsafe fn __children(&self) -> &Self::Children {
        T::__children(self)
    }
}
impl<T: IKeyExprTreeNode<Weight>, Weight> IKeyExprTreeNode<Weight> for &mut T {}
impl<T: IKeyExprTreeNode<Weight>, Weight> UIKeyExprTreeNode<Weight> for &mut T {
    type Parent = T::Parent;
    unsafe fn __parent(&self) -> Option<&Self::Parent> {
        T::__parent(self)
    }
    unsafe fn __keyexpr(&self) -> OwnedKeyExpr {
        T::__keyexpr(self)
    }
    unsafe fn __weight(&self) -> Option<&Weight> {
        T::__weight(self)
    }

    type Child = T::Child;
    type Children = T::Children;

    unsafe fn __children(&self) -> &Self::Children {
        T::__children(self)
    }
}
impl<T: IKeyExprTreeNode<Weight>, Weight> IKeyExprTreeNode<Weight> for Box<T> {}
impl<T: IKeyExprTreeNode<Weight>, Weight> UIKeyExprTreeNode<Weight> for Box<T> {
    type Parent = T::Parent;
    unsafe fn __parent(&self) -> Option<&Self::Parent> {
        T::__parent(self)
    }
    unsafe fn __keyexpr(&self) -> OwnedKeyExpr {
        T::__keyexpr(self)
    }
    unsafe fn __weight(&self) -> Option<&Weight> {
        T::__weight(self)
    }

    type Child = T::Child;
    type Children = T::Children;

    unsafe fn __children(&self) -> &Self::Children {
        T::__children(self)
    }
}

impl<T: IKeyExprTreeNodeMut<Weight>, Weight> IKeyExprTreeNodeMut<Weight> for &mut T {
    fn parent_mut(&mut self) -> Option<&mut Self::Parent> {
        T::parent_mut(self)
    }
    fn weight_mut(&mut self) -> Option<&mut Weight> {
        T::weight_mut(self)
    }
    fn take_weight(&mut self) -> Option<Weight> {
        T::take_weight(self)
    }
    fn insert_weight(&mut self, weight: Weight) -> Option<Weight> {
        T::insert_weight(self, weight)
    }

    fn children_mut(&mut self) -> &mut Self::Children {
        T::children_mut(self)
    }
}

impl<T: IKeyExprTreeNodeMut<Weight>, Weight> IKeyExprTreeNodeMut<Weight> for Box<T> {
    fn parent_mut(&mut self) -> Option<&mut Self::Parent> {
        T::parent_mut(self)
    }
    fn weight_mut(&mut self) -> Option<&mut Weight> {
        T::weight_mut(self)
    }
    fn take_weight(&mut self) -> Option<Weight> {
        T::take_weight(self)
    }
    fn insert_weight(&mut self, weight: Weight) -> Option<Weight> {
        T::insert_weight(self, weight)
    }

    fn children_mut(&mut self) -> &mut Self::Children {
        T::children_mut(self)
    }
}

impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> IKeyExprTreeNode<Weight>
    for (&TokenCell<T, Token>, &Token)
{
}
impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> UIKeyExprTreeNode<Weight>
    for (&TokenCell<T, Token>, &Token)
{
    type Parent = T::Parent;
    unsafe fn __parent(&self) -> Option<&Self::Parent> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__parent()
    }
    unsafe fn __keyexpr(&self) -> OwnedKeyExpr {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__keyexpr()
    }
    unsafe fn __weight(&self) -> Option<&Weight> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__weight()
    }

    type Child = T::Child;
    type Children = T::Children;

    unsafe fn __children(&self) -> &Self::Children {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__children()
    }
}

impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> IKeyExprTreeNode<Weight>
    for (&TokenCell<T, Token>, &mut Token)
{
}
impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> UIKeyExprTreeNode<Weight>
    for (&TokenCell<T, Token>, &mut Token)
{
    type Parent = T::Parent;
    unsafe fn __parent(&self) -> Option<&Self::Parent> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__parent()
    }
    unsafe fn __keyexpr(&self) -> OwnedKeyExpr {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__keyexpr()
    }
    unsafe fn __weight(&self) -> Option<&Weight> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__weight()
    }

    type Child = T::Child;
    type Children = T::Children;

    unsafe fn __children(&self) -> &Self::Children {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__children()
    }
}

impl<T: IKeyExprTreeNodeMut<Weight>, Weight, Token: TokenTrait> IKeyExprTreeNodeMut<Weight>
    for (&TokenCell<T, Token>, &mut Token)
{
    fn parent_mut(&mut self) -> Option<&mut Self::Parent> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .parent_mut()
    }
    fn weight_mut(&mut self) -> Option<&mut Weight> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .weight_mut()
    }
    fn take_weight(&mut self) -> Option<Weight> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .take_weight()
    }
    fn insert_weight(&mut self, weight: Weight) -> Option<Weight> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .insert_weight(weight)
    }

    fn children_mut(&mut self) -> &mut Self::Children {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .children_mut()
    }
}

impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> IKeyExprTreeNode<Weight>
    for (&Arc<TokenCell<T, Token>>, &Token)
{
}
impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> UIKeyExprTreeNode<Weight>
    for (&Arc<TokenCell<T, Token>>, &Token)
{
    type Parent = T::Parent;
    unsafe fn __parent(&self) -> Option<&Self::Parent> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__parent()
    }
    unsafe fn __keyexpr(&self) -> OwnedKeyExpr {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__keyexpr()
    }
    unsafe fn __weight(&self) -> Option<&Weight> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__weight()
    }

    type Child = T::Child;
    type Children = T::Children;

    unsafe fn __children(&self) -> &Self::Children {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__children()
    }
}

impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> IKeyExprTreeNode<Weight>
    for (&Arc<TokenCell<T, Token>>, &mut Token)
{
}
impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> UIKeyExprTreeNode<Weight>
    for (&Arc<TokenCell<T, Token>>, &mut Token)
{
    type Parent = T::Parent;
    unsafe fn __parent(&self) -> Option<&Self::Parent> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__parent()
    }
    unsafe fn __keyexpr(&self) -> OwnedKeyExpr {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__keyexpr()
    }
    unsafe fn __weight(&self) -> Option<&Weight> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__weight()
    }

    type Child = T::Child;
    type Children = T::Children;

    unsafe fn __children(&self) -> &Self::Children {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__children()
    }
}

impl<T: IKeyExprTreeNodeMut<Weight>, Weight, Token: TokenTrait> IKeyExprTreeNodeMut<Weight>
    for (&Arc<TokenCell<T, Token>>, &mut Token)
{
    fn parent_mut(&mut self) -> Option<&mut Self::Parent> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .parent_mut()
    }
    fn weight_mut(&mut self) -> Option<&mut Weight> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .weight_mut()
    }
    fn take_weight(&mut self) -> Option<Weight> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .take_weight()
    }
    fn insert_weight(&mut self, weight: Weight) -> Option<Weight> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .insert_weight(weight)
    }

    fn children_mut(&mut self) -> &mut Self::Children {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .children_mut()
    }
}

use crate::keyexpr_tree::arc_tree::Tokenized;
impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> IKeyExprTreeNode<Weight>
    for Tokenized<&TokenCell<T, Token>, &Token>
{
}
impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> UIKeyExprTreeNode<Weight>
    for Tokenized<&TokenCell<T, Token>, &Token>
{
    type Parent = T::Parent;
    unsafe fn __parent(&self) -> Option<&Self::Parent> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__parent()
    }
    unsafe fn __keyexpr(&self) -> OwnedKeyExpr {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__keyexpr()
    }
    unsafe fn __weight(&self) -> Option<&Weight> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__weight()
    }

    type Child = T::Child;
    type Children = T::Children;

    unsafe fn __children(&self) -> &Self::Children {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__children()
    }
}

impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> IKeyExprTreeNode<Weight>
    for Tokenized<&TokenCell<T, Token>, &mut Token>
{
}
impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> UIKeyExprTreeNode<Weight>
    for Tokenized<&TokenCell<T, Token>, &mut Token>
{
    type Parent = T::Parent;
    unsafe fn __parent(&self) -> Option<&Self::Parent> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__parent()
    }
    unsafe fn __keyexpr(&self) -> OwnedKeyExpr {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__keyexpr()
    }
    unsafe fn __weight(&self) -> Option<&Weight> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__weight()
    }

    type Child = T::Child;
    type Children = T::Children;

    unsafe fn __children(&self) -> &Self::Children {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__children()
    }
}

impl<T: IKeyExprTreeNodeMut<Weight>, Weight, Token: TokenTrait> IKeyExprTreeNodeMut<Weight>
    for Tokenized<&TokenCell<T, Token>, &mut Token>
{
    fn parent_mut(&mut self) -> Option<&mut Self::Parent> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .parent_mut()
    }
    fn weight_mut(&mut self) -> Option<&mut Weight> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .weight_mut()
    }
    fn take_weight(&mut self) -> Option<Weight> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .take_weight()
    }
    fn insert_weight(&mut self, weight: Weight) -> Option<Weight> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .insert_weight(weight)
    }

    fn children_mut(&mut self) -> &mut Self::Children {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .children_mut()
    }
}

impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> IKeyExprTreeNode<Weight>
    for Tokenized<&Arc<TokenCell<T, Token>>, &Token>
{
}
impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> UIKeyExprTreeNode<Weight>
    for Tokenized<&Arc<TokenCell<T, Token>>, &Token>
{
    type Parent = T::Parent;
    unsafe fn __parent(&self) -> Option<&Self::Parent> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__parent()
    }
    unsafe fn __keyexpr(&self) -> OwnedKeyExpr {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__keyexpr()
    }
    unsafe fn __weight(&self) -> Option<&Weight> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__weight()
    }

    type Child = T::Child;
    type Children = T::Children;

    unsafe fn __children(&self) -> &Self::Children {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__children()
    }
}

impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> IKeyExprTreeNode<Weight>
    for Tokenized<&Arc<TokenCell<T, Token>>, &mut Token>
{
}
impl<T: IKeyExprTreeNode<Weight>, Weight, Token: TokenTrait> UIKeyExprTreeNode<Weight>
    for Tokenized<&Arc<TokenCell<T, Token>>, &mut Token>
{
    type Parent = T::Parent;
    unsafe fn __parent(&self) -> Option<&Self::Parent> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__parent()
    }
    unsafe fn __keyexpr(&self) -> OwnedKeyExpr {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__keyexpr()
    }
    unsafe fn __weight(&self) -> Option<&Weight> {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__weight()
    }

    type Child = T::Child;
    type Children = T::Children;

    unsafe fn __children(&self) -> &Self::Children {
        self.0
            .try_borrow(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .__children()
    }
}

impl<T: IKeyExprTreeNodeMut<Weight>, Weight, Token: TokenTrait> IKeyExprTreeNodeMut<Weight>
    for Tokenized<&Arc<TokenCell<T, Token>>, &mut Token>
{
    fn parent_mut(&mut self) -> Option<&mut Self::Parent> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .parent_mut()
    }
    fn weight_mut(&mut self) -> Option<&mut Weight> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .weight_mut()
    }
    fn take_weight(&mut self) -> Option<Weight> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .take_weight()
    }
    fn insert_weight(&mut self, weight: Weight) -> Option<Weight> {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .insert_weight(weight)
    }

    fn children_mut(&mut self) -> &mut Self::Children {
        self.0
            .try_borrow_mut(self.1)
            .unwrap_or_else(|_| panic!("Used wrong token to access TokenCell"))
            .children_mut()
    }
}
