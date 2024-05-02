//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

use alloc::vec::Vec;

use zenoh_result::unlikely;

use crate::keyexpr_tree::*;

struct StackFrame<'a, Children: IChildrenProvider<Node>, Node: UIKeyExprTreeNode<Weight>, Weight>
where
    Children::Assoc: IChildren<Node> + 'a,
    <Children::Assoc as IChildren<Node>>::Node: 'a,
{
    iterator: <Children::Assoc as IChildren<Node>>::Iter<'a>,
    start: usize,
    end: usize,
    _marker: core::marker::PhantomData<Weight>,
}
pub struct Intersection<
    'a,
    Children: IChildrenProvider<Node>,
    Node: UIKeyExprTreeNode<Weight>,
    Weight,
> where
    Children::Assoc: IChildren<Node> + 'a,
{
    key: &'a keyexpr,
    ke_indices: Vec<usize>,
    iterators: Vec<StackFrame<'a, Children, Node, Weight>>,
}

impl<'a, Children: IChildrenProvider<Node>, Node: UIKeyExprTreeNode<Weight>, Weight>
    Intersection<'a, Children, Node, Weight>
where
    Children::Assoc: IChildren<Node> + 'a,
{
    pub(crate) fn new(children: &'a Children::Assoc, key: &'a keyexpr) -> Self {
        let mut ke_indices = Vec::with_capacity(32);
        ke_indices.push(0);
        let mut iterators = Vec::with_capacity(16);
        iterators.push(StackFrame {
            iterator: children.children(),
            start: 0,
            end: 1,
            _marker: Default::default(),
        });
        Self {
            key,
            ke_indices,
            iterators,
        }
    }
}

impl<
        'a,
        Children: IChildrenProvider<Node>,
        Node: UIKeyExprTreeNode<Weight, Children = Children::Assoc> + 'a,
        Weight,
    > Iterator for Intersection<'a, Children, Node, Weight>
where
    Children::Assoc: IChildren<Node> + 'a,
{
    type Item = &'a Node;
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
                            if new_end == new_start || self.ke_indices[new_end - 1] < index {
                                self.ke_indices.push(index);
                                new_end += 1;
                            }
                        };
                    }
                    let chunk = node.chunk();
                    let chunk_is_verbatim = chunk.first_byte() == b'@';
                    if unlikely(chunk.as_bytes() == b"**") {
                        // If the current node is `**`, it is guaranteed to match...
                        node_matches = true;
                        // and may consume any number of chunks from the KE
                        push!(self.ke_indices[*start]);
                        if self.key.len() != self.ke_indices[*start] {
                            if self.key.as_bytes()[self.ke_indices[*start]] != b'@' {
                                for i in self.ke_indices[*start]..self.key.len() {
                                    if self.key.as_bytes()[i] == b'/' {
                                        push!(i + 1);
                                        if self.key.as_bytes()[i + 1] == b'@' {
                                            node_matches = false; // ...unless the KE contains a verbatim chunk.
                                            break;
                                        }
                                    }
                                }
                            } else {
                                node_matches = false;
                            }
                        }
                    } else {
                        // The current node is not `**`
                        // For all candidate chunks of the KE
                        for i in *start..*end {
                            // construct that chunk, while checking whether or not it's the last one
                            let kec_start = self.ke_indices[i];
                            if unlikely(kec_start == self.key.len()) {
                                break;
                            }
                            let key = &self.key.as_bytes()[kec_start..];
                            match key.iter().position(|&c| c == b'/') {
                                Some(kec_end) => {
                                    // If we aren't in the last chunk
                                    let subkey =
                                        unsafe { keyexpr::from_slice_unchecked(&key[..kec_end]) };
                                    if unlikely(subkey.as_bytes() == b"**") {
                                        if !chunk_is_verbatim {
                                            // If the query chunk is `**`:
                                            // children will have to process it again
                                            push!(kec_start);
                                        }
                                        // and we need to process this chunk as if the `**` wasn't there,
                                        // but with the knowledge that the next chunk won't be `**`.
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
                                                    push!(self.key.len());
                                                    node_matches = true;
                                                }
                                            }
                                        }
                                    } else if chunk.intersects(subkey) {
                                        push!(kec_start + kec_end + 1);
                                    }
                                }
                                None => {
                                    // If it's the last chunk of the query, check whether it's `**`
                                    let key = unsafe { keyexpr::from_slice_unchecked(key) };
                                    if unlikely(key.as_bytes() == b"**") && !chunk_is_verbatim {
                                        // If yes, it automatically matches, and must be reused from now on for iteration.
                                        push!(kec_start);
                                        node_matches = true;
                                    } else if chunk.intersects(key) {
                                        // else, if it intersects with the chunk, make sure the children of the node
                                        // are searched for `**`
                                        push!(self.key.len());
                                        node_matches = true;
                                    }
                                }
                            }
                        }
                    }
                    // If new progress points have been set
                    if new_end != new_start {
                        // Check if any of them is `**`, as this would mean a match
                        for &i in &self.ke_indices[new_start..new_end] {
                            if &self.key.as_bytes()[i..] == b"**" {
                                node_matches = true;
                                break;
                            }
                        }
                        // Prepare the next children
                        let iterator = unsafe { node.as_node().__children() }.children();
                        self.iterators.push(StackFrame {
                            iterator,
                            start: new_start,
                            end: new_end,
                            _marker: Default::default(),
                        })
                    }
                    // yield the node if a match was found
                    if node_matches {
                        return Some(node.as_node());
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
struct StackFrameMut<'a, Children: IChildrenProvider<Node>, Node: IKeyExprTreeNode<Weight>, Weight>
where
    Children::Assoc: IChildren<Node> + 'a,
    <Children::Assoc as IChildren<Node>>::Node: 'a,
{
    iterator: <Children::Assoc as IChildren<Node>>::IterMut<'a>,
    start: usize,
    end: usize,
    _marker: core::marker::PhantomData<Weight>,
}

pub struct IntersectionMut<
    'a,
    Children: IChildrenProvider<Node>,
    Node: IKeyExprTreeNode<Weight>,
    Weight,
> where
    Children::Assoc: IChildren<Node> + 'a,
{
    key: &'a keyexpr,
    ke_indices: Vec<usize>,
    iterators: Vec<StackFrameMut<'a, Children, Node, Weight>>,
}

impl<'a, Children: IChildrenProvider<Node>, Node: IKeyExprTreeNode<Weight>, Weight>
    IntersectionMut<'a, Children, Node, Weight>
where
    Children::Assoc: IChildren<Node> + 'a,
{
    pub(crate) fn new(children: &'a mut Children::Assoc, key: &'a keyexpr) -> Self {
        let mut ke_indices = Vec::with_capacity(32);
        ke_indices.push(0);
        let mut iterators = Vec::with_capacity(16);
        iterators.push(StackFrameMut {
            iterator: children.children_mut(),
            start: 0,
            end: 1,
            _marker: Default::default(),
        });
        Self {
            key,
            ke_indices,
            iterators,
        }
    }
}

impl<
        'a,
        Children: IChildrenProvider<Node>,
        Node: IKeyExprTreeNodeMut<Weight, Children = Children::Assoc> + 'a,
        Weight,
    > Iterator for IntersectionMut<'a, Children, Node, Weight>
where
    Children::Assoc: IChildren<Node> + 'a,
{
    type Item = &'a mut <Children::Assoc as IChildren<Node>>::Node;
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
                            if new_end == new_start || self.ke_indices[new_end - 1] < index {
                                self.ke_indices.push(index);
                                new_end += 1;
                            }
                        };
                    }
                    let chunk = node.chunk();
                    let chunk_is_verbatim = chunk.first_byte() == b'@';
                    if unlikely(chunk.as_bytes() == b"**") {
                        // If the current node is `**`, it is guaranteed to match...
                        node_matches = true;
                        // and may consume any number of chunks from the KE
                        push!(self.ke_indices[*start]);
                        if self.key.len() != self.ke_indices[*start] {
                            if self.key.as_bytes()[self.ke_indices[*start]] != b'@' {
                                for i in self.ke_indices[*start]..self.key.len() {
                                    if self.key.as_bytes()[i] == b'/' {
                                        push!(i + 1);
                                        if self.key.as_bytes()[i + 1] == b'@' {
                                            node_matches = false; // ...unless the KE contains a verbatim chunk.
                                            break;
                                        }
                                    }
                                }
                            } else {
                                node_matches = false;
                            }
                        }
                    } else {
                        // The current node is not `**`
                        // For all candidate chunks of the KE
                        for i in *start..*end {
                            // construct that chunk, while checking whether or not it's the last one
                            let kec_start = self.ke_indices[i];
                            if unlikely(kec_start == self.key.len()) {
                                break;
                            }
                            let key = &self.key.as_bytes()[kec_start..];
                            match key.iter().position(|&c| c == b'/') {
                                Some(kec_end) => {
                                    // If we aren't in the last chunk
                                    let subkey =
                                        unsafe { keyexpr::from_slice_unchecked(&key[..kec_end]) };
                                    if unlikely(subkey.as_bytes() == b"**") {
                                        if !chunk_is_verbatim {
                                            // If the query chunk is `**`:
                                            // children will have to process it again
                                            push!(kec_start);
                                        }
                                        // and we need to process this chunk as if the `**` wasn't there,
                                        // but with the knowledge that the next chunk won't be `**`.
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
                                                    push!(self.key.len());
                                                    node_matches = true;
                                                }
                                            }
                                        }
                                    } else if chunk.intersects(subkey) {
                                        push!(kec_start + kec_end + 1);
                                    }
                                }
                                None => {
                                    // If it's the last chunk of the query, check whether it's `**`
                                    let key = unsafe { keyexpr::from_slice_unchecked(key) };
                                    if unlikely(key.as_bytes() == b"**") && !chunk_is_verbatim {
                                        // If yes, it automatically matches, and must be reused from now on for iteration.
                                        push!(kec_start);
                                        node_matches = true;
                                    } else if chunk.intersects(key) {
                                        // else, if it intersects with the chunk, make sure the children of the node
                                        // are searched for `**`
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
