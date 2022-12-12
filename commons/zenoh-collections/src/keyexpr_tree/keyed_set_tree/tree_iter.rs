use std::num::NonZeroUsize;

use super::*;
pub struct TreeIter<'a, Children: ChunkMapType<Node>, Node: IKeyExprTreeNode<Weight>, Weight>
where
    Children::Assoc: ChunkMap<Node> + 'a,
    <Children::Assoc as ChunkMap<Node>>::Node: 'a,
{
    iterators: Vec<<Children::Assoc as ChunkMap<Node>>::Iter<'a>>,
    _marker: std::marker::PhantomData<Weight>,
}

impl<'a, Children: ChunkMapType<Node>, Node: IKeyExprTreeNode<Weight>, Weight>
    TreeIter<'a, Children, Node, Weight>
where
    Children::Assoc: ChunkMap<Node> + 'a,
{
    pub(crate) fn new(children: &'a Children::Assoc) -> Self {
        Self {
            iterators: vec![children.children()],
            _marker: Default::default(),
        }
    }
    pub fn with_depth(self) -> DepthInstrumented<Self> {
        DepthInstrumented(self)
    }
}

impl<
        'a,
        Children: ChunkMapType<Node>,
        Node: IKeyExprTreeNode<Weight, Children = Children::Assoc> + 'a,
        Weight,
    > Iterator for TreeIter<'a, Children, Node, Weight>
where
    Children::Assoc: ChunkMap<Node> + 'a,
{
    type Item = &'a <Children::Assoc as ChunkMap<Node>>::Node;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.iterators.last_mut()?.next() {
                Some(node) => {
                    let iterator = unsafe { &*(node.as_node() as *const Node) }
                        .children()
                        .children();
                    self.iterators.push(iterator);
                    return Some(node);
                }
                None => {
                    self.iterators.pop();
                }
            }
        }
    }
}
pub struct TreeIterMut<'a, Children: ChunkMapType<Node>, Node: IKeyExprTreeNode<Weight>, Weight>
where
    Children::Assoc: ChunkMap<Node> + 'a,
    <Children::Assoc as ChunkMap<Node>>::Node: 'a,
{
    iterators: Vec<<Children::Assoc as ChunkMap<Node>>::IterMut<'a>>,
    _marker: std::marker::PhantomData<Weight>,
}

impl<'a, Children: ChunkMapType<Node>, Node: IKeyExprTreeNode<Weight>, Weight>
    TreeIterMut<'a, Children, Node, Weight>
where
    Children::Assoc: ChunkMap<Node> + 'a,
{
    pub(crate) fn new(children: &'a mut Children::Assoc) -> Self {
        Self {
            iterators: vec![children.children_mut()],
            _marker: Default::default(),
        }
    }
}

impl<
        'a,
        Children: ChunkMapType<Node>,
        Node: IKeyExprTreeNode<Weight, Children = Children::Assoc> + 'a,
        Weight,
    > Iterator for TreeIterMut<'a, Children, Node, Weight>
where
    Children::Assoc: ChunkMap<Node> + 'a,
{
    type Item = &'a mut <Children::Assoc as ChunkMap<Node>>::Node;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.iterators.last_mut()?.next() {
                Some(node) => {
                    let iterator = unsafe { &mut *(node.as_node_mut() as *mut Node) }
                        .children_mut()
                        .children_mut();
                    self.iterators.push(iterator);
                    return Some(node);
                }
                None => {
                    self.iterators.pop();
                }
            }
        }
    }
}

pub struct DepthInstrumented<T>(T);
impl<
        'a,
        Children: ChunkMapType<Node>,
        Node: IKeyExprTreeNode<Weight, Children = Children::Assoc> + 'a,
        Weight,
    > Iterator for DepthInstrumented<TreeIter<'a, Children, Node, Weight>>
where
    Children::Assoc: ChunkMap<Node> + 'a,
{
    type Item = (NonZeroUsize, &'a <Children::Assoc as ChunkMap<Node>>::Node);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let depth = self.0.iterators.len();
            match self.0.iterators.last_mut()?.next() {
                Some(node) => {
                    let iterator = unsafe { &*(node.as_node() as *const Node) }
                        .children()
                        .children();
                    self.0.iterators.push(iterator);
                    return Some((unsafe { NonZeroUsize::new_unchecked(depth) }, node));
                }
                None => {
                    self.0.iterators.pop();
                }
            }
        }
    }
}
