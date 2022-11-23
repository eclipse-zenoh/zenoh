use zenoh_protocol_core::key_expr::{keyexpr, OwnedKeyExpr};

use self::box_tree::ChunkMap;

pub type KeyExprTree<Weight> = box_tree::KeyExprTree<Weight, box_tree::DefaultChunkMapProvider>;
pub trait IKeyExprTree<Weight> {
    type Node: IKeyExprTreeNode<Weight>;
    fn node(&self, at: &keyexpr) -> Option<&Self::Node>;
    fn node_mut(&mut self, at: &keyexpr) -> Option<&mut Self::Node>;
    fn node_mut_or_create(&mut self, at: &keyexpr) -> &mut Self::Node;
    type TreeIter<'a>: Iterator<Item = &'a Self::Node>
    where
        Self: 'a,
        Self::Node: 'a;
    fn tree_iter<'a>(&'a self) -> Self::TreeIter<'a>;
    // type TreeIterMut<'a>: Iterator<Item = &'a mut Self::Node>
    // where
    //     Self: 'a,
    //     Self::Node: 'a;
    // fn tree_iter_mut<'a>(&'a self) -> Self::TreeIterMut<'a>;
    // type Intersection<'a>: Iterator<Item = &'a Self::Node>
    // where
    //     Self: 'a,
    //     Self::Node: 'a;
    // fn matching_nodes<'a>(&'a self, ke: &'a keyexpr) -> Self::Intersection<'a>;
    // type IntersectionMut<'a>: Iterator<Item = &'a mut Self::Node>
    // where
    //     Self: 'a,
    //     Self::Node: 'a;
    // fn matching_nodes_mut<'a>(&'a mut self, ke: &'a keyexpr) -> Self::IntersectionMut<'a>;
}
pub trait IKeyExprTreeNode<Weight> {
    fn parent(&self) -> Option<&Self>;
    fn parent_mut(&mut self) -> Option<&mut Self>;
    fn child_at(&self, chunk: &keyexpr) -> Option<&Self>;
    fn child_at_mut(&mut self, chunk: &keyexpr) -> Option<&mut Self>;
    fn keyexpr(&self) -> OwnedKeyExpr;
    fn weight(&self) -> Option<&Weight>;
    fn weight_mut(&mut self) -> Option<&mut Weight>;
    fn take_weight(&mut self) -> Option<Weight>;
}
pub mod box_tree;
