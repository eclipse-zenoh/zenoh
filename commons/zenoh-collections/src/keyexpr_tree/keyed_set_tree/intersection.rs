use zenoh_core::unlikely;

use super::*;
struct StackFrame<'a, Children: ChunkMapType<Node>, Node: IKeyExprTreeNode<Weight>, Weight>
where
    Children::Assoc: ChunkMap<Node> + 'a,
    <Children::Assoc as ChunkMap<Node>>::Node: 'a,
{
    iterator: <Children::Assoc as ChunkMap<Node>>::Iter<'a>,
    start: usize,
    end: usize,
    _marker: std::marker::PhantomData<Weight>,
}
pub struct Intersection<'a, Children: ChunkMapType<Node>, Node: IKeyExprTreeNode<Weight>, Weight>
where
    Children::Assoc: ChunkMap<Node> + 'a,
{
    key: &'a keyexpr,
    ke_indices: Vec<usize>,
    iterators: Vec<StackFrame<'a, Children, Node, Weight>>,
}

impl<'a, Children: ChunkMapType<Node>, Node: IKeyExprTreeNode<Weight>, Weight>
    Intersection<'a, Children, Node, Weight>
where
    Children::Assoc: ChunkMap<Node> + 'a,
{
    pub(crate) fn new(children: &'a Children::Assoc, key: &'a keyexpr) -> Self {
        Self {
            key,
            ke_indices: vec![0],
            iterators: vec![StackFrame {
                iterator: children.children(),
                start: 0,
                end: 1,
                _marker: Default::default(),
            }],
        }
    }
}

impl<
        'a,
        Children: ChunkMapType<Node>,
        Node: IKeyExprTreeNode<Weight, Children = Children::Assoc> + 'a,
        Weight,
    > Iterator for Intersection<'a, Children, Node, Weight>
where
    Children::Assoc: ChunkMap<Node> + 'a,
{
    type Item = &'a <Children::Assoc as ChunkMap<Node>>::Node;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let StackFrame {
                iterator,
                start,
                end,
                _marker,
            } = self.iterators.last_mut()?;
            match iterator.next() {
                Some(node) => {
                    let mut node_matches = false;
                    let new_start = *end;
                    let mut new_end = *end;
                    macro_rules! push {
                        ($index: expr) => {
                            let index = $index;
                            if new_end == new_start
                                || self.ke_indices[new_start..new_end]
                                    .iter()
                                    .rev()
                                    .all(|c| *c < index)
                            {
                                self.ke_indices.push(index);
                                new_end += 1;
                            }
                        };
                    }
                    let chunk = node.chunk();
                    if unlikely(chunk == "**") {
                        node_matches = true;
                        push!(self.ke_indices[*start]);
                        for i in self.ke_indices[*start]..self.key.len() {
                            if self.key.as_bytes()[i] == b'/' {
                                push!(i + 1);
                            }
                        }
                    } else {
                        for i in *start..*end {
                            let kec_start = self.ke_indices[i];
                            if kec_start == self.key.len() {
                                break;
                            }
                            let key = &self.key.as_bytes()[kec_start..];
                            match key.iter().position(|&c| c == b'/') {
                                Some(kec_end) => {
                                    let subkey =
                                        unsafe { keyexpr::from_slice_unchecked(&key[..kec_end]) };
                                    if unlikely(subkey == "**") {
                                        push!(kec_start);
                                        push!(kec_start + kec_end + 1);
                                        let post_key = &key[kec_end + 1..];
                                        match post_key.iter().position(|&c| c == b'/') {
                                            Some(sec_end) => {
                                                let post_key = unsafe {
                                                    keyexpr::from_slice_unchecked(
                                                        &post_key[..sec_end],
                                                    )
                                                };
                                                if post_key.intersects(chunk) {
                                                    push!(kec_start + kec_end + sec_end + 2);
                                                }
                                            }
                                            None => {
                                                if unsafe {
                                                    keyexpr::from_slice_unchecked(post_key)
                                                }
                                                .intersects(chunk)
                                                {
                                                    node_matches = true;
                                                }
                                            }
                                        }
                                    } else if chunk.intersects(subkey) {
                                        push!(kec_start + kec_end + 1);
                                    }
                                }
                                None => {
                                    let key = unsafe { keyexpr::from_slice_unchecked(key) };
                                    if unlikely(key == "**") {
                                        push!(kec_start);
                                        node_matches = true;
                                    } else if chunk.intersects(key) {
                                        push!(self.key.len());
                                        node_matches = true;
                                    }
                                }
                            }
                        }
                    }
                    if new_end != new_start {
                        for &i in &self.ke_indices[new_start..new_end] {
                            if &self.key.as_bytes()[i..] == b"**" {
                                node_matches = true;
                                break;
                            }
                        }
                        let iterator = unsafe { &*(node.as_node() as *const Node) }
                            .children()
                            .children();
                        self.iterators.push(StackFrame {
                            iterator,
                            start: new_start,
                            end: new_end,
                            _marker: Default::default(),
                        })
                    }
                    if node_matches {
                        return Some(node);
                    }
                }
                None => {
                    if let Some(StackFrame { start, .. }) = self.iterators.pop() {
                        self.ke_indices.truncate(start);
                    }
                }
            }
        }
    }
}
struct StackFrameMut<'a, Children: ChunkMapType<Node>, Node: IKeyExprTreeNode<Weight>, Weight>
where
    Children::Assoc: ChunkMap<Node> + 'a,
    <Children::Assoc as ChunkMap<Node>>::Node: 'a,
{
    iterator: <Children::Assoc as ChunkMap<Node>>::IterMut<'a>,
    start: usize,
    end: usize,
    _marker: std::marker::PhantomData<Weight>,
}

pub struct IntersectionMut<'a, Children: ChunkMapType<Node>, Node: IKeyExprTreeNode<Weight>, Weight>
where
    Children::Assoc: ChunkMap<Node> + 'a,
{
    key: &'a keyexpr,
    ke_indices: Vec<usize>,
    iterators: Vec<StackFrameMut<'a, Children, Node, Weight>>,
}

impl<'a, Children: ChunkMapType<Node>, Node: IKeyExprTreeNode<Weight>, Weight>
    IntersectionMut<'a, Children, Node, Weight>
where
    Children::Assoc: ChunkMap<Node> + 'a,
{
    pub(crate) fn new(children: &'a mut Children::Assoc, key: &'a keyexpr) -> Self {
        Self {
            key,
            ke_indices: vec![0],
            iterators: vec![StackFrameMut {
                iterator: children.children_mut(),
                start: 0,
                end: 1,
                _marker: Default::default(),
            }],
        }
    }
}

impl<
        'a,
        Children: ChunkMapType<Node>,
        Node: IKeyExprTreeNode<Weight, Children = Children::Assoc> + 'a,
        Weight,
    > Iterator for IntersectionMut<'a, Children, Node, Weight>
where
    Children::Assoc: ChunkMap<Node> + 'a,
{
    type Item = &'a mut <Children::Assoc as ChunkMap<Node>>::Node;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let StackFrameMut {
                iterator,
                start,
                end,
                _marker,
            } = self.iterators.last_mut()?;
            match iterator.next() {
                Some(node) => {
                    let mut node_matches = false;
                    let new_start = *end;
                    let mut new_end = *end;
                    macro_rules! push {
                        ($index: expr) => {
                            let index = $index;
                            if new_end == new_start
                                || self.ke_indices[new_start..new_end]
                                    .iter()
                                    .rev()
                                    .all(|c| *c < index)
                            {
                                self.ke_indices.push(index);
                                new_end += 1;
                            }
                        };
                    }
                    let chunk = node.chunk();
                    if unlikely(chunk == "**") {
                        node_matches = true;
                        push!(self.ke_indices[*start]);
                        for i in self.ke_indices[*start]..self.key.len() {
                            if self.key.as_bytes()[i] == b'/' {
                                push!(i + 1);
                            }
                        }
                    } else {
                        for i in *start..*end {
                            let kec_start = self.ke_indices[i];
                            if kec_start == self.key.len() {
                                break;
                            }
                            let key = &self.key.as_bytes()[kec_start..];
                            match key.iter().position(|&c| c == b'/') {
                                Some(kec_end) => {
                                    let subkey =
                                        unsafe { keyexpr::from_slice_unchecked(&key[..kec_end]) };
                                    if unlikely(subkey == "**") {
                                        push!(kec_start);
                                        push!(kec_start + kec_end + 1);
                                        let post_key = &key[kec_end + 1..];
                                        match post_key.iter().position(|&c| c == b'/') {
                                            Some(sec_end) => {
                                                let post_key = unsafe {
                                                    keyexpr::from_slice_unchecked(
                                                        &post_key[..sec_end],
                                                    )
                                                };
                                                if post_key.intersects(chunk) {
                                                    push!(kec_start + kec_end + sec_end + 2);
                                                }
                                            }
                                            None => {
                                                if unsafe {
                                                    keyexpr::from_slice_unchecked(post_key)
                                                }
                                                .intersects(chunk)
                                                {
                                                    node_matches = true;
                                                }
                                            }
                                        }
                                    } else if chunk.intersects(subkey) {
                                        push!(kec_start + kec_end + 1);
                                    }
                                }
                                None => {
                                    let key = unsafe { keyexpr::from_slice_unchecked(key) };
                                    if unlikely(key == "**") {
                                        push!(kec_start);
                                        node_matches = true;
                                    } else if chunk.intersects(key) {
                                        push!(self.key.len());
                                        node_matches = true;
                                    }
                                }
                            }
                        }
                    }
                    if new_end != new_start {
                        for &i in &self.ke_indices[new_start..new_end] {
                            if &self.key.as_bytes()[i..] == b"**" {
                                node_matches = true;
                                break;
                            }
                        }
                        let iterator = unsafe { &mut *(node.as_node_mut() as *mut Node) }
                            .children_mut()
                            .children_mut();
                        self.iterators.push(StackFrameMut {
                            iterator,
                            start: new_start,
                            end: new_end,
                            _marker: Default::default(),
                        })
                    }
                    if node_matches {
                        return Some(node);
                    }
                }
                None => {
                    if let Some(StackFrameMut { start, .. }) = self.iterators.pop() {
                        self.ke_indices.truncate(start);
                    }
                }
            }
        }
    }
}
