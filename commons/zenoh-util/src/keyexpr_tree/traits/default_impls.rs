use super::*;

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
impl<T: HasChunk> HasChunk for Box<T> {
    fn chunk(&self) -> &keyexpr {
        T::chunk(self)
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
impl<T: IKeyExprTreeNode<Weight>, Weight> IKeyExprTreeNode<Weight> for Box<T> {
    type Parent = T::Parent;
    fn parent(&self) -> Option<&Self::Parent> {
        T::parent(self)
    }
    fn parent_mut(&mut self) -> Option<&mut Self::Parent> {
        T::parent_mut(self)
    }
    fn keyexpr(&self) -> OwnedKeyExpr {
        T::keyexpr(self)
    }
    fn weight(&self) -> Option<&Weight> {
        T::weight(self)
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

    type Child = T::Child;
    type Children = T::Children;

    fn children(&self) -> &Self::Children {
        T::children(self)
    }
    fn children_mut(&mut self) -> &mut Self::Children {
        T::children_mut(self)
    }
}
