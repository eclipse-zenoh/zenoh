use std::collections::{hash_map::Entry, HashMap};

use crate::keyexpr_tree::*;

pub struct HashMapProvider;
impl<T: 'static> IChildrenProvider<T> for HashMapProvider {
    type Assoc = Vec<T>;
}

impl<'a: 'b, 'b, T: HasChunk> IEntry<'a, 'b, T> for Entry<'a, OwnedKeyExpr, T> {
    fn get_or_insert_with<F: FnOnce(&'b keyexpr) -> T>(self, f: F) -> &'a mut T {
        match self {
            Entry::Vacant(entry) => {
                let value = unsafe { f(std::mem::transmute::<&keyexpr, &keyexpr>(entry.key())) };
                entry.insert(value)
            }
            Entry::Occupied(v) => v.into_mut(),
        }
    }
}

impl<T: HasChunk + AsNode<T> + AsNodeMut<T> + 'static> IChildren<T> for HashMap<OwnedKeyExpr, T> {
    type Node = T;
    fn child_at(&self, chunk: &keyexpr) -> Option<&T> {
        self.get(chunk)
    }
    fn child_at_mut(&mut self, chunk: &keyexpr) -> Option<&mut T> {
        self.get_mut(chunk)
    }
    fn remove(&mut self, chunk: &keyexpr) -> Option<Self::Node> {
        self.remove(chunk)
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
    type Entry<'a, 'b> = Entry<'a, OwnedKeyExpr, T> where Self: 'a, 'a: 'b, T: 'b;
    fn entry<'a, 'b>(&'a mut self, chunk: &'b keyexpr) -> Self::Entry<'a, 'b>
    where
        Self: 'a,
        'a: 'b,
        T: 'b,
    {
        self.entry(chunk.into())
    }

    type Iter<'a> = std::collections::hash_map::Values<'a, OwnedKeyExpr, T> where Self: 'a;
    fn children<'a>(&'a self) -> Self::Iter<'a>
    where
        Self: 'a,
    {
        self.values()
    }

    type IterMut<'a> = std::collections::hash_map::ValuesMut<'a, OwnedKeyExpr, T>
where
    Self: 'a;

    fn children_mut<'a>(&'a mut self) -> Self::IterMut<'a>
    where
        Self: 'a,
    {
        self.values_mut()
    }

    fn filter_out<F: FnMut(&mut T) -> bool>(&mut self, predicate: &mut F) {
        self.retain(|_, v| predicate(v));
    }

    type Intersection<'a> = super::FilterMap<std::collections::hash_map::Iter<'a, OwnedKeyExpr, T>, super::Intersection<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn intersection<'a>(&'a self, key: &'a keyexpr) -> Self::Intersection<'a> {
        super::FilterMap::new(self.iter(), super::Intersection(key))
    }
    type IntersectionMut<'a> = super::FilterMap<std::collections::hash_map::IterMut<'a, OwnedKeyExpr, T>, super::Intersection<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn intersection_mut<'a>(&'a mut self, key: &'a keyexpr) -> Self::IntersectionMut<'a> {
        super::FilterMap::new(self.iter_mut(), super::Intersection(key))
    }
    type Inclusion<'a> = super::FilterMap<std::collections::hash_map::Iter<'a, OwnedKeyExpr, T>, super::Inclusion<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn inclusion<'a>(&'a self, key: &'a keyexpr) -> Self::Inclusion<'a> {
        super::FilterMap::new(self.iter(), super::Inclusion(key))
    }
    type InclusionMut<'a> = super::FilterMap<std::collections::hash_map::IterMut<'a, OwnedKeyExpr, T>, super::Inclusion<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn inclusion_mut<'a>(&'a mut self, key: &'a keyexpr) -> Self::InclusionMut<'a> {
        super::FilterMap::new(self.iter_mut(), super::Inclusion(key))
    }
}
