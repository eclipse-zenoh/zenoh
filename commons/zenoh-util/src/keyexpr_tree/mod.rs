pub use box_tree::KeBoxTree;
use zenoh_protocol::core::key_expr::{keyexpr, OwnedKeyExpr};
pub mod traits;
pub use traits::*;

pub mod arc_tree;
pub mod box_tree;
pub mod impls;
pub use impls::DefaultChildrenProvider;
mod iters;
pub use iters::*;

#[cfg(test)]
mod test;

pub trait IWildness: 'static {
    fn non_wild() -> Self;
    fn get(&self) -> bool;
    fn set(&mut self, wildness: bool) -> bool;
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NonWild;
impl IWildness for NonWild {
    fn non_wild() -> Self {
        NonWild
    }
    fn get(&self) -> bool {
        false
    }
    fn set(&mut self, wildness: bool) -> bool {
        if wildness {
            panic!("Attempted to set NonWild to wild, which breaks its contract. You likely attempted to insert a wild key into a `NonWild` tree. Use `bool` instead to make wildness determined at runtime.")
        }
        false
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnknownWildness;
impl IWildness for UnknownWildness {
    fn non_wild() -> Self {
        UnknownWildness
    }
    fn get(&self) -> bool {
        true
    }
    fn set(&mut self, _wildness: bool) -> bool {
        true
    }
}
impl IWildness for bool {
    fn non_wild() -> Self {
        false
    }
    fn get(&self) -> bool {
        *self
    }
    fn set(&mut self, wildness: bool) -> bool {
        std::mem::replace(self, wildness)
    }
}
