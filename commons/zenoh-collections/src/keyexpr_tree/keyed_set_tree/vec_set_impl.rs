use zenoh_core::unlikely;

use super::*;

pub struct VecSetProvider;
impl<T: 'static> ChunkMapType<T> for VecSetProvider {
    type Assoc = Vec<T>;
}

impl<'a, 'b, T: HasChunk> IEntry<'a, 'b, T> for Entry<'a, 'b, T> {
    fn get_or_insert_with<F: FnOnce(&'b keyexpr) -> T>(self, f: F) -> &'a mut T {
        match self {
            Entry::Vacant(vec, key) => {
                vec.push(f(key));
                vec.last_mut().unwrap()
            }
            Entry::Occupied(v) => v,
        }
    }
}

pub enum Entry<'a, 'b, T> {
    Vacant(&'a mut Vec<T>, &'b keyexpr),
    Occupied(&'a mut T),
}
impl<T: HasChunk + AsNode<T> + AsNodeMut<T> + 'static> ChunkMap<T> for Vec<T> {
    type Node = T;
    fn child_at(&self, chunk: &keyexpr) -> Option<&T> {
        self.iter().find(|t| unlikely(t.chunk() == chunk))
    }
    fn child_at_mut(&mut self, chunk: &keyexpr) -> Option<&mut T> {
        self.iter_mut().find(|t| unlikely(t.chunk() == chunk))
    }
    type Entry<'a, 'b> = Entry<'a, 'b, T> where Self: 'a + 'b, T: 'b;
    fn entry<'a, 'b>(&'a mut self, chunk: &'b keyexpr) -> Self::Entry<'a, 'b>
    where
        Self: 'a + 'b,
        T: 'b,
    {
        let this = unsafe { &mut *(self as *mut Self) };
        match self.child_at_mut(chunk) {
            Some(entry) => Entry::Occupied(entry),
            None => Entry::Vacant(this, chunk),
        }
    }

    type Iter<'a> = std::slice::Iter<'a, T> where Self: 'a;
    fn children<'a>(&'a self) -> Self::Iter<'a>
    where
        Self: 'a,
    {
        self.iter()
    }

    type IterMut<'a> = std::slice::IterMut<'a, T>
where
    Self: 'a;

    fn children_mut<'a>(&'a mut self) -> Self::IterMut<'a>
    where
        Self: 'a,
    {
        self.iter_mut()
    }
}
