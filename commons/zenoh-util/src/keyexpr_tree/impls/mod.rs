pub use hashmap_impl::HashMapProvider;
pub use keyed_set_impl::KeyedSetProvider;
pub use vec_set_impl::VecSetProvider;
mod hashmap_impl;
mod keyed_set_impl;
mod vec_set_impl;

pub type DefaultChildrenProvider = KeyedSetProvider;
