use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt::{self, Debug};
use std::hash::Hash;

use slab::Slab;

/// A deduplicated sequence of calls to tracked functions.
///
/// This is a more elaborate version of `Constraint` that is suitable for
/// insertion into a `CallTree`.
pub struct CallSequence<C> {
    /// The raw calls. In order, but deduplicated via the `map`.
    vec: Vec<Option<(C, u128)>>,
    /// A map from hashes of calls to the indices in the vector.
    map: HashMap<u128, usize>,
    /// A cursor for iteration in `Self::next`.
    cursor: usize,
}

impl<C> CallSequence<C> {
    /// Creates an empty sequence.
    pub fn new() -> Self {
        Self { vec: Vec::new(), map: HashMap::new(), cursor: 0 }
    }
}

impl<C: Hash> CallSequence<C> {
    /// Inserts a pair of a call and its return hash.
    ///
    /// Returns true when the pair was indeed inserted and false if the call was
    /// deduplicated.
    pub fn insert(&mut self, call: C, ret: u128) -> bool {
        match self.map.entry(crate::hash::hash(&call)) {
            Entry::Vacant(entry) => {
                let i = self.vec.len();
                self.vec.push(Some((call, ret)));
                entry.insert(i);
                true
            }
            Entry::Occupied(entry) => {
                #[cfg(debug_assertions)]
                if let Some((_, ret2)) = &self.vec[*entry.get()] {
                    if ret != *ret2 {
                        panic!(
                            "comemo: found differing return values. \
                             is there an impure tracked function?"
                        )
                    }
                }
                false
            }
        }
    }

    /// Retrieves the next call in order.
    fn next(&mut self) -> Option<(C, u128)> {
        while self.cursor < self.vec.len() {
            if let Some(pair) = self.vec[self.cursor].take() {
                return Some(pair);
            }
            self.cursor += 1;
        }
        None
    }

    /// Retrieves the return hash of an arbitrary upcoming call. Removes the
    /// call from the sequence; it will not be yielded by `next()` anymore.
    fn extract(&mut self, call: &C) -> Option<u128> {
        let h = crate::hash::hash(&call);
        let i = *self.map.get(&h)?;
        let res = self.vec[i].take().map(|(_, ret)| ret);
        debug_assert!(self.cursor <= i || res.is_none());
        res
    }
}

impl<C> Default for CallSequence<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Hash> FromIterator<(C, u128)> for CallSequence<C> {
    fn from_iter<T: IntoIterator<Item = (C, u128)>>(iter: T) -> Self {
        let mut seq = CallSequence::new();
        for (call, ret) in iter {
            seq.insert(call, ret);
        }
        seq
    }
}

/// A tree data structure that associates a value with a key hash and a sequence
/// of (call, return hash) pairs.
///
/// Allows to efficiently query for a value for which every call in the sequence
/// yielded the same return hash as a given oracle function will yield for that
/// call.
pub struct CallTree<C, T> {
    /// Inner nodes, storing calls.
    inner: Slab<InnerNode<C>>,
    /// Leaf nodes, directly storing outputs.
    leaves: Slab<LeafNode<T>>,
    /// The initial node for the given key hash.
    start: HashMap<u128, NodeId>,
    /// Maps from parent nodes to child nodes. The key is a pair of an inner
    /// node ID and a return hash for that call. The value is the node to
    /// transition to.
    edges: HashMap<(InnerId, u128), NodeId>,
}

/// An inner node in the call tree.
struct InnerNode<C> {
    /// The call at this node.
    call: C,
    /// How many children the node has. If this reaches zero, the node is
    /// deleted.
    children: usize,
    /// The node's parent.
    parent: Option<InnerId>,
}

/// A leaf node in the call tree.
struct LeafNode<T> {
    /// The value.
    value: T,
    /// The node's parent.
    parent: Option<InnerId>,
}

impl<C, T> CallTree<C, T> {
    /// Creates an empty call tree.
    pub fn new() -> Self {
        Self {
            inner: Slab::new(),
            leaves: Slab::new(),
            edges: HashMap::new(),
            start: HashMap::new(),
        }
    }
}

impl<C: Hash, T> CallTree<C, T> {
    /// Retrieves the output value for the given key and oracle.
    pub fn get(&self, key: u128, mut oracle: impl FnMut(&C) -> u128) -> Option<&T> {
        let mut cursor = *self.start.get(&key)?;
        loop {
            match cursor.kind() {
                NodeIdKind::Leaf(id) => {
                    return Some(&self.leaves[id].value);
                }
                NodeIdKind::Inner(id) => {
                    let call = &self.inner[id].call;
                    let ret = oracle(call);
                    cursor = *self.edges.get(&(id, ret))?;
                }
            }
        }
    }

    /// Inserts a key and a call sequence and its associated value into the
    /// tree.
    ///
    /// See the documentation of [`InsertError`] for more details on when this
    /// can fail.
    pub fn insert(
        &mut self,
        key: u128,
        mut sequence: CallSequence<C>,
        value: T,
    ) -> Result<(), InsertError> {
        let mut cursor = self.start.get(&key).copied();
        let mut predecessor = None;

        loop {
            if predecessor.is_none()
                && let Some(pos) = cursor
            {
                let NodeIdKind::Inner(id) = pos.kind() else {
                    return Err(InsertError::AlreadyExists);
                };

                let call = &self.inner[id].call;
                let Some(ret) = sequence.extract(call) else {
                    return Err(InsertError::MissingCall);
                };

                let pair = (id, ret);
                if let Some(&next) = self.edges.get(&pair) {
                    // We are still on an existing path.
                    cursor = Some(next);
                } else {
                    // We are now starting to build a new path in the tree.
                    predecessor = Some(pair);
                }
            } else {
                // We are adding a new node to the tree for the next call in the
                // sequence.
                let Some((call, ret)) = sequence.next() else { break };

                let new_inner_id = self.inner.insert(InnerNode {
                    call,
                    children: 0,
                    parent: predecessor.map(|(id, _)| id),
                });
                let new_id = NodeId::inner(new_inner_id);
                self.link(cursor.is_none(), key, predecessor.take(), new_id);

                predecessor = Some((new_inner_id, ret));
                cursor = Some(new_id);
            }
        }

        if predecessor.is_none() && cursor.is_some() {
            return Err(InsertError::AlreadyExists);
        }

        let target = NodeId::leaf(
            self.leaves
                .insert(LeafNode { value, parent: predecessor.map(|(id, _)| id) }),
        );
        self.link(cursor.is_none(), key, predecessor, target);

        Ok(())
    }

    /// Creates a new link between two nodes.
    fn link(
        &mut self,
        at_start: bool,
        key: u128,
        from: Option<(InnerId, u128)>,
        to: NodeId,
    ) {
        if at_start {
            self.start.insert(key, to);
        }
        if let Some(pair) = from {
            self.inner[pair.0].children += 1;
            self.edges.insert(pair, to);
        }
    }

    /// Removes all call sequences from the tree for whose values the predicate
    /// returns `false`.
    pub fn retain(&mut self, mut f: impl FnMut(&mut T) -> bool) {
        // Prune from the leafs upwards, starting with the outputs.
        self.leaves.retain(|_, node| {
            let keep = f(&mut node.value);
            if !keep {
                // Delete parents iteratively while we are the only child.
                let mut parent = node.parent;
                while let Some(inner_id) = parent {
                    let node = &mut self.inner[inner_id];
                    if node.children > 1 {
                        node.children -= 1;
                        break;
                    } else {
                        parent = self.inner[inner_id].parent;
                        self.inner.remove(inner_id);
                    }
                }
            }
            keep
        });

        // Checks whether the given node survived the pruning.
        let exists = |node: NodeId| match node.kind() {
            NodeIdKind::Inner(id) => self.inner.contains(id),
            NodeIdKind::Leaf(id) => self.leaves.contains(id),
        };

        // Prune edges.
        self.edges.retain(|_, node| exists(*node));
        self.start.retain(|_, node| exists(*node));
    }

    /// Checks a few invariants of the data structure.
    #[cfg(test)]
    fn assert_consistency(&self) {
        let exists = |node: NodeId| match node.kind() {
            NodeIdKind::Inner(id) => self.inner.contains(id),
            NodeIdKind::Leaf(id) => self.leaves.contains(id),
        };

        for &node in self.start.values() {
            assert!(exists(node));
        }

        for (&(inner_id, _), &node) in &self.edges {
            assert!(exists(node));
            assert!(self.inner.contains(inner_id));
        }
    }
}

impl<C: Debug, T: Debug> Debug for CallTree<C, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (&(inner_id, ret), next) in &self.edges {
            let call = &self.inner[inner_id].call;
            write!(f, "[{inner_id}] ({call:?}, {ret:?}) -> ")?;
            match next.kind() {
                NodeIdKind::Inner(id) => writeln!(f, "{id}")?,
                NodeIdKind::Leaf(id) => writeln!(f, "{:?}", &self.leaves[id].value)?,
            }
        }
        Ok(())
    }
}

impl<C, T> Default for CallTree<C, T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Identifies a node in the call tree.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
struct NodeId(isize);

impl NodeId {
    /// An inner with an index pointing into the `calls` slab allocator.
    fn inner(i: usize) -> Self {
        Self(i as isize)
    }

    /// A leaf node with an index pointing into the `output` slab allocator.
    fn leaf(i: usize) -> Self {
        Self(-(i as isize) - 1)
    }

    /// Makes this encoded node available as an enum for matching.
    fn kind(self) -> NodeIdKind {
        if self.0 >= 0 {
            NodeIdKind::Inner(self.0 as usize)
        } else {
            NodeIdKind::Leaf((-self.0) as usize - 1)
        }
    }
}

/// An unpacked representation of a `NodeId`.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum NodeIdKind {
    Inner(InnerId),
    Leaf(LeafId),
}

type InnerId = usize;
type LeafId = usize;

/// An error that can occur during insertion of a call sequence into the call
/// tree.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum InsertError {
    /// A call sequence that is a prefix of the used one was already inserted.
    AlreadyExists,
    /// The calls from the sequence gave results matching the first N ones from
    /// an existing sequence S in the tree, but the N+1th call from S does not
    /// exist in the given `sequence`. This points towards non-determinism in
    /// the memoized function the `sequence` belongs to.
    MissingCall,
}

#[cfg(test)]
mod tests {
    use quickcheck::Arbitrary;

    use super::*;

    #[test]
    fn test_call_tree() {
        test_ops([
            Op::Insert(0, vec![('a', 10), ('b', 15)], "first"),
            Op::Insert(0, vec![('a', 10), ('b', 20)], "second"),
            Op::Insert(0, vec![('a', 15), ('c', 15)], "third"),
        ]);
        test_ops([
            Op::Insert(0, vec![('a', 10), ('b', 15)], "first"),
            Op::Insert(0, vec![('a', 10), ('c', 15), ('b', 20)], "second"),
            Op::Insert(0, vec![('a', 15), ('b', 30), ('c', 15)], "third"),
            Op::Manual(|tree| {
                assert_eq!(tree.inner.len(), 5);
                assert_eq!(tree.leaves.len(), 3);
                assert_eq!(tree.edges.len(), 7);
                assert_eq!(tree.start.len(), 1);
            }),
            Op::Retain(Box::new(|v| *v == "second")),
            Op::Manual(|tree| {
                assert_eq!(tree.inner.len(), 3);
                assert_eq!(tree.leaves.len(), 1);
                assert_eq!(tree.edges.len(), 3);
                assert_eq!(tree.start.len(), 1);
            }),
        ]);
    }

    #[quickcheck_macros::quickcheck]
    fn test_arbitrary_quickcheck(ops: Vec<ArbitraryOp>) {
        test_ops(
            std::iter::once(Op::IgnoreInsertErrors)
                .chain(ops.into_iter().map(ArbitraryOp::into_op)),
        );
    }

    #[derive(Debug, Clone)]
    enum ArbitraryOp {
        Insert(u128, Vec<u16>, u8),
        Retain(u8),
    }

    impl ArbitraryOp {
        fn into_op(self) -> Op<u64, u8> {
            match self {
                Self::Insert(key, nums, output) => {
                    let mut state = 50;
                    Op::Insert(
                        key,
                        nums.iter()
                            .map(move |&v| {
                                let pair = (state, v as u128);
                                state += 1 + v as u64;
                                pair
                            })
                            .collect(),
                        output,
                    )
                }
                Self::Retain(mid) => Op::Retain(Box::new(move |v| *v > mid)),
            }
        }
    }

    impl Arbitrary for ArbitraryOp {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            match g.choose(&[0, 1]) {
                Some(0) => Self::Insert(
                    Arbitrary::arbitrary(g),
                    Arbitrary::arbitrary(g),
                    Arbitrary::arbitrary(g),
                ),
                _ => Self::Retain(Arbitrary::arbitrary(g)),
            }
        }
    }

    enum Op<C, T> {
        IgnoreInsertErrors,
        Insert(u128, Vec<(C, u128)>, T),
        Retain(Box<dyn Fn(&T) -> bool>),
        Manual(fn(&mut CallTree<C, T>)),
    }

    #[track_caller]
    fn test_ops<C, T>(ops: impl IntoIterator<Item = Op<C, T>>)
    where
        C: Clone + Hash + Eq,
        T: Debug + PartialEq + Clone,
    {
        let mut tree = CallTree::new();
        let mut kept = Vec::<(u128, HashMap<C, u128>, T)>::new();
        let mut ignore_insert_errors = false;

        for op in ops {
            match op {
                Op::IgnoreInsertErrors => ignore_insert_errors = true,
                Op::Insert(key, seq, value) => {
                    match tree.insert(key, seq.iter().cloned().collect(), value.clone()) {
                        Ok(()) => kept.push((
                            key,
                            seq.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                            value.clone(),
                        )),
                        Err(_) if ignore_insert_errors => {}
                        Err(e) => panic!("{e:?}"),
                    }
                }
                Op::Retain(f) => {
                    tree.retain(|v| f(v));
                    kept.retain_mut(|(key, map, v)| {
                        let keep = f(v);
                        if !keep {
                            assert_eq!(tree.get(*key, |s| map[s]), None);
                        }
                        keep
                    });
                }
                Op::Manual(f) => f(&mut tree),
            }

            tree.assert_consistency();

            for (key, map, value) in &kept {
                assert_eq!(tree.get(*key, |s| map[s]), Some(value));
            }
        }
    }
}
