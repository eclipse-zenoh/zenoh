use super::*;
use keyed_set::{KeyExtractor, KeyedSet};

pub struct KeyedSetProvider;
impl<T: 'static> ChunkMapType<T> for KeyedSetProvider {
    type Assoc = KeyedSet<T, ChunkExtractor>;
}
#[derive(Debug, Default, Clone, Copy)]
pub struct ChunkExtractor;
impl<'a, T: HasChunk> KeyExtractor<'a, T> for ChunkExtractor {
    type Key = &'a keyexpr;
    fn extract(&self, from: &'a T) -> Self::Key {
        from.chunk()
    }
}
impl<'a, 'b, T: HasChunk> IEntry<'a, 'b, T>
    for keyed_set::Entry<'a, T, ChunkExtractor, &'b keyexpr>
{
    fn get_or_insert_with<F: FnOnce(&'b keyexpr) -> T>(self, f: F) -> &'a mut T {
        Self::get_or_insert_with(self, f)
    }
}

impl<T: HasChunk + AsNode<T> + AsNodeMut<T>> ChunkMap<T> for KeyedSet<T, ChunkExtractor> {
    type Node = T;
    fn child_at(&self, chunk: &keyexpr) -> Option<&T> {
        self.get(&chunk)
    }
    fn child_at_mut(&mut self, chunk: &keyexpr) -> Option<&mut T> {
        self.get_mut_unguarded(&chunk)
    }
    type Entry<'a, 'b> = keyed_set::Entry<'a, T, ChunkExtractor, &'b keyexpr> where Self: 'a + 'b, T: 'b;
    fn entry<'a, 'b>(&'a mut self, chunk: &'b keyexpr) -> Self::Entry<'a, 'b>
    where
        Self: 'a + 'b,
        T: 'b,
    {
        self.entry(chunk)
    }

    type Iter<'a> = keyed_set::Iter<'a, T> where Self: 'a;
    fn children<'a>(&'a self) -> Self::Iter<'a>
    where
        Self: 'a,
    {
        self.iter()
    }

    type IterMut<'a> = keyed_set::IterMut<'a, T>
where
    Self: 'a;

    fn children_mut<'a>(&'a mut self) -> Self::IterMut<'a>
    where
        Self: 'a,
    {
        self.iter_mut()
    }
}
