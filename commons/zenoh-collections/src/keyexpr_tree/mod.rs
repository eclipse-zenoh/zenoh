use std::marker::PhantomData;

use zenoh_protocol_core::key_expr::OwnedKeyExpr;

pub trait IKeyExprTree<Weight> {}

pub mod box_tree;
