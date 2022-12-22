pub use impls::KeyExprTree;
use zenoh_protocol_core::key_expr::{keyexpr, OwnedKeyExpr};

pub trait IKeyExprTree<Weight> {
    type Node: IKeyExprTreeNode<Weight>;
    fn node(&self, at: &keyexpr) -> Option<&Self::Node>;
    fn node_mut(&mut self, at: &keyexpr) -> Option<&mut Self::Node>;
    fn remove(&mut self, at: &keyexpr) -> Option<Weight>;
    fn node_mut_or_create(&mut self, at: &keyexpr) -> &mut Self::Node;
    type TreeIterItem<'a>
    where
        Self: 'a;
    type TreeIter<'a>: Iterator<Item = Self::TreeIterItem<'a>>
    where
        Self: 'a;
    fn tree_iter(&self) -> Self::TreeIter<'_>;
    type TreeIterItemMut<'a>
    where
        Self: 'a;
    type TreeIterMut<'a>: Iterator<Item = Self::TreeIterItemMut<'a>>
    where
        Self: 'a;
    fn tree_iter_mut(&mut self) -> Self::TreeIterMut<'_>;
    type IntersectionItem<'a>
    where
        Self: 'a;
    type Intersection<'a>: Iterator<Item = Self::IntersectionItem<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn intersecting_nodes<'a>(&'a self, key: &'a keyexpr) -> Self::Intersection<'a>;
    type IntersectionItemMut<'a>
    where
        Self: 'a;
    type IntersectionMut<'a>: Iterator<Item = Self::IntersectionItemMut<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn intersecting_nodes_mut<'a>(&'a mut self, key: &'a keyexpr) -> Self::IntersectionMut<'a>;
    type InclusionItem<'a>
    where
        Self: 'a;
    type Inclusion<'a>: Iterator<Item = Self::InclusionItem<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn included_nodes<'a>(&'a self, key: &'a keyexpr) -> Self::Inclusion<'a>;
    type InclusionItemMut<'a>
    where
        Self: 'a;
    type InclusionMut<'a>: Iterator<Item = Self::InclusionItemMut<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn included_nodes_mut<'a>(&'a mut self, key: &'a keyexpr) -> Self::InclusionMut<'a>;
    fn prune_where<F: FnMut(&mut Self::Node) -> bool>(&mut self, predicate: F);
}
type Keys<I, Item> = std::iter::FilterMap<I, fn(Item) -> Option<OwnedKeyExpr>>;
pub trait IKeyExprTreeExt<Weight>: IKeyExprTree<Weight> {
    fn weight_at(&self, at: &keyexpr) -> Option<&Weight> {
        self.node(at)
            .and_then(<Self::Node as IKeyExprTreeNode<Weight>>::weight)
    }
    fn weight_at_mut(&mut self, at: &keyexpr) -> Option<&mut Weight> {
        self.node_mut(at)
            .and_then(<Self::Node as IKeyExprTreeNode<Weight>>::weight_mut)
    }
    fn insert(&mut self, at: &keyexpr, weight: Weight) -> Option<Weight> {
        self.node_mut_or_create(at).insert_weight(weight)
    }
    fn intersecting_keys<'a>(
        &'a self,
        key: &'a keyexpr,
    ) -> Keys<Self::Intersection<'a>, Self::IntersectionItem<'a>>
    where
        Self::IntersectionItem<'a>: AsNode<Self::Node>,
        Self::Node: IKeyExprTreeNode<Weight>,
    {
        self.intersecting_nodes(key)
            .filter_map(filter_map_weighted_node_to_key)
    }
    fn included_keys<'a>(
        &'a self,
        key: &'a keyexpr,
    ) -> Keys<Self::Inclusion<'a>, Self::InclusionItem<'a>>
    where
        Self::InclusionItem<'a>: AsNode<Self::Node>,
        Self::Node: IKeyExprTreeNode<Weight>,
    {
        self.included_nodes(key)
            .filter_map(filter_map_weighted_node_to_key)
    }
    fn prune(&mut self) {
        self.prune_where(|node| node.weight().is_none())
    }
}
impl<Weight, T: IKeyExprTree<Weight>> IKeyExprTreeExt<Weight> for T {}
fn filter_map_weighted_node_to_key<N: IKeyExprTreeNode<W>, I: AsNode<N>, W>(
    item: I,
) -> Option<OwnedKeyExpr> {
    let node = item.as_node();
    node.weight().is_some().then(|| node.keyexpr())
}
pub trait IKeyExprTreeNode<Weight> {
    type Parent;
    fn parent(&self) -> Option<&Self::Parent>;
    fn parent_mut(&mut self) -> Option<&mut Self::Parent>;
    fn keyexpr(&self) -> OwnedKeyExpr;
    fn weight(&self) -> Option<&Weight>;
    fn weight_mut(&mut self) -> Option<&mut Weight>;
    fn take_weight(&mut self) -> Option<Weight>;
    fn insert_weight(&mut self, weight: Weight) -> Option<Weight>;
    type Child;
    type Children: ChunkMap<Self::Child>;
    fn children(&self) -> &Self::Children;
    fn children_mut(&mut self) -> &mut Self::Children;
}

pub trait ChunkMapType<T> {
    type Assoc: Default + 'static;
}

pub trait ChunkMap<T: ?Sized> {
    type Node: HasChunk + AsNode<T> + AsNodeMut<T>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn child_at<'a, 'b>(&'a self, chunk: &'b keyexpr) -> Option<&'a Self::Node>;
    fn child_at_mut<'a, 'b>(&'a mut self, chunk: &'b keyexpr) -> Option<&'a mut Self::Node>;
    type Entry<'a, 'b>: IEntry<'a, 'b, T>
    where
        Self: 'a + 'b,
        T: 'b;
    fn remove(&mut self, chunk: &keyexpr) -> Option<Self::Node>;
    fn entry<'a, 'b>(&'a mut self, chunk: &'b keyexpr) -> Self::Entry<'a, 'b>
    where
        Self: 'a + 'b,
        T: 'b;
    type Iter<'a>: Iterator<Item = &'a Self::Node>
    where
        Self: 'a,
        Self::Node: 'a;
    fn children<'a>(&'a self) -> Self::Iter<'a>
    where
        Self: 'a;
    type IterMut<'a>: Iterator<Item = &'a mut Self::Node>
    where
        Self: 'a,
        Self::Node: 'a;
    fn children_mut<'a>(&'a mut self) -> Self::IterMut<'a>
    where
        Self: 'a;
    fn filter_out<F: FnMut(&mut T) -> bool>(&mut self, predicate: &mut F);
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
impl<T: HasChunk> HasChunk for Box<T> {
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

pub mod impls;

#[cfg(test)]
mod test;

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
