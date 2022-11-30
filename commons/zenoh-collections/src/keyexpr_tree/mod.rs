use zenoh_protocol_core::key_expr::{keyexpr, OwnedKeyExpr};

pub type KeyExprTree<Weight> =
    keyed_set_tree::KeyExprTree<Weight, keyed_set_tree::DefaultChunkMapProvider>;
pub trait IKeyExprTree<Weight> {
    type Node: IKeyExprTreeNode<Weight>;
    fn node(&self, at: &keyexpr) -> Option<&Self::Node>;
    fn node_mut(&mut self, at: &keyexpr) -> Option<&mut Self::Node>;
    fn node_mut_or_create(&mut self, at: &keyexpr) -> &mut Self::Node;
    fn insert(&mut self, at: &keyexpr, weight: Weight) -> Option<Weight> {
        self.node_mut_or_create(at).insert_weight(weight)
    }
    type TreeIterItem<'a>
    where
        Self: 'a;
    type TreeIter<'a>: Iterator<Item = Self::TreeIterItem<'a>>
    where
        Self: 'a;
    fn tree_iter(&self) -> Self::TreeIter<'_>;
    // type TreeIterMut<'a>: Iterator<Item = &'a mut Self::Node>
    // where
    //     Self: 'a,
    //     Self::Node: 'a;
    // fn tree_iter_mut<'a>(&'a self) -> Self::TreeIterMut<'a>;
    type IntersectionItem<'a>
    where
        Self: 'a;
    type Intersection<'a>: Iterator<Item = Self::IntersectionItem<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn intersecting_nodes<'a>(&'a self, ke: &'a keyexpr) -> Self::Intersection<'a>;
    // type IntersectionMut<'a>: Iterator<Item = &'a mut Self::Node>
    // where
    //     Self: 'a,
    //     Self::Node: 'a;
    // fn matching_nodes_mut<'a>(&'a mut self, ke: &'a keyexpr) -> Self::IntersectionMut<'a>;
}
pub trait IKeyExprTreeNode<Weight> {
    fn parent(&self) -> Option<&Self>;
    fn parent_mut(&mut self) -> Option<&mut Self>;
    fn keyexpr(&self) -> OwnedKeyExpr;
    fn weight(&self) -> Option<&Weight>;
    fn weight_mut(&mut self) -> Option<&mut Weight>;
    fn take_weight(&mut self) -> Option<Weight>;
    fn insert_weight(&mut self, weight: Weight) -> Option<Weight>;
    type Children: ChunkMap<Self>;
    fn children(&self) -> &Self::Children;
    fn children_mut(&mut self) -> &mut Self::Children;
}

pub trait ChunkMapType<T> {
    type Assoc: Default + 'static;
}

pub trait ChunkMap<T: ?Sized> {
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
    type IterItemMut<'a>: HasChunk + AsNodeMut<T>
    where
        Self: 'a;
    type IterMut<'a>: Iterator<Item = Self::IterItemMut<'a>>
    where
        Self: 'a;
    fn children_mut<'a>(&'a mut self) -> Self::IterMut<'a>
    where
        Self: 'a;
}
pub trait IEntry<'a, 'b, T: ?Sized> {
    fn get_or_insert_with<F: FnOnce(&'b keyexpr) -> T>(self, f: F) -> &'a mut T;
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
pub trait AsNode<T: ?Sized> {
    fn as_node(&self) -> &T;
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
pub trait AsNodeMut<T: ?Sized>: AsNode<T> {
    fn as_node_mut(&mut self) -> &mut T;
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

pub mod keyed_set_tree;

#[cfg(test)]
mod test;
