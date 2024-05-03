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
pub struct Inclusion<'a, Children: IChildrenProvider<Node>, Node: UIKeyExprTreeNode<Weight>, Weight>
where
    Children::Assoc: IChildren<Node> + 'a,
{
    key: &'a keyexpr,
    ke_indices: Vec<usize>,
    iterators: Vec<StackFrame<'a, Children, Node, Weight>>,
}

impl<'a, Children: IChildrenProvider<Node>, Node: UIKeyExprTreeNode<Weight>, Weight>
    Inclusion<'a, Children, Node, Weight>
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
    > Iterator for Inclusion<'a, Children, Node, Weight>
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
                    let chunk_is_verbatim = chunk.first_byte() == b'@';
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
                                    if !chunk_is_verbatim {
                                        push!(kec_start);
                                        push!(kec_start + kec_end + 1);
                                    }
                                    let post_key = &key[kec_end + 1..];
                                    match post_key.iter().position(|&c| c == b'/') {
                                        Some(sec_end) => {
                                            let post_key = unsafe {
                                                keyexpr::from_slice_unchecked(&post_key[..sec_end])
                                            };
                                            if post_key.includes(chunk) {
                                                push!(kec_start + kec_end + sec_end + 2);
                                            }
                                        }
                                        None => {
                                            if unsafe { keyexpr::from_slice_unchecked(post_key) }
                                                .includes(chunk)
                                            {
                                                node_matches = true;
                                            }
                                        }
                                    }
                                } else if subkey.includes(chunk) {
                                    push!(kec_start + kec_end + 1);
                                }
                            }
                            None => {
                                let key = unsafe { keyexpr::from_slice_unchecked(key) };
                                if unlikely(key == "**") && chunk.first_byte() != b'@' {
                                    push!(kec_start);
                                    node_matches = true;
                                } else if key.includes(chunk) {
                                    push!(self.key.len());
                                    node_matches = true;
                                }
                            }
                        }
                    }
                    if new_end > new_start {
                        for &i in &self.ke_indices[new_start..new_end] {
                            if &self.key.as_bytes()[i..] == b"**" {
                                node_matches = true;
                                break;
                            }
                        }
                        let iterator = unsafe { node.as_node().__children() }.children();
                        self.iterators.push(StackFrame {
                            iterator,
                            start: new_start,
                            end: new_end,
                            _marker: Default::default(),
                        })
                    }
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
struct StackFrameMut<'a, Children: IChildrenProvider<Node>, Node: UIKeyExprTreeNode<Weight>, Weight>
where
    Children::Assoc: IChildren<Node> + 'a,
    <Children::Assoc as IChildren<Node>>::Node: 'a,
{
    iterator: <Children::Assoc as IChildren<Node>>::IterMut<'a>,
    start: usize,
    end: usize,
    _marker: core::marker::PhantomData<Weight>,
}

pub struct InclusionMut<
    'a,
    Children: IChildrenProvider<Node>,
    Node: UIKeyExprTreeNode<Weight>,
    Weight,
> where
    Children::Assoc: IChildren<Node> + 'a,
{
    key: &'a keyexpr,
    ke_indices: Vec<usize>,
    iterators: Vec<StackFrameMut<'a, Children, Node, Weight>>,
}

impl<'a, Children: IChildrenProvider<Node>, Node: UIKeyExprTreeNode<Weight>, Weight>
    InclusionMut<'a, Children, Node, Weight>
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
    > Iterator for InclusionMut<'a, Children, Node, Weight>
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
                    let chunk_is_verbatim = chunk.first_byte() == b'@';
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
                                    if !chunk_is_verbatim {
                                        push!(kec_start);
                                        push!(kec_start + kec_end + 1);
                                    }
                                    let post_key = &key[kec_end + 1..];
                                    match post_key.iter().position(|&c| c == b'/') {
                                        Some(sec_end) => {
                                            let post_key = unsafe {
                                                keyexpr::from_slice_unchecked(&post_key[..sec_end])
                                            };
                                            if post_key.includes(chunk) {
                                                push!(kec_start + kec_end + sec_end + 2);
                                            }
                                        }
                                        None => {
                                            if unsafe { keyexpr::from_slice_unchecked(post_key) }
                                                .includes(chunk)
                                            {
                                                node_matches = true;
                                            }
                                        }
                                    }
                                } else if subkey.includes(chunk) {
                                    push!(kec_start + kec_end + 1);
                                }
                            }
                            None => {
                                let key = unsafe { keyexpr::from_slice_unchecked(key) };
                                if unlikely(key == "**") && chunk.first_byte() != b'@' {
                                    push!(kec_start);
                                    node_matches = true;
                                } else if key.includes(chunk) {
                                    push!(self.key.len());
                                    node_matches = true;
                                }
                            }
                        }
                    }
                    if new_end > new_start {
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
