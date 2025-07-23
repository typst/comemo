use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt::{self, Debug};
use std::hash::Hash;

pub struct LookaheadSequence<Q, A> {
    vec: Vec<Option<(Q, A)>>,
    map: HashMap<u128, usize>,
    cursor: usize,
}

impl<Q, A> LookaheadSequence<Q, A> {
    pub fn new() -> Self {
        Self { vec: Vec::new(), map: HashMap::new(), cursor: 0 }
    }

    pub fn next(&mut self) -> Option<(Q, A)> {
        while self.cursor < self.vec.len() {
            if let Some(pair) = self.vec[self.cursor].take() {
                return Some(pair);
            }
            self.cursor += 1;
        }
        None
    }
}

impl<Q: Hash, A: Hash + Eq> LookaheadSequence<Q, A> {
    pub fn insert(&mut self, q: Q, a: A) {
        let h = crate::constraint::hash(&q);
        match self.map.entry(h) {
            Entry::Vacant(entry) => {
                let i = self.vec.len();
                self.vec.push(Some((q, a)));
                entry.insert(i);
            }
            Entry::Occupied(entry) =>
            {
                #[cfg(debug_assertions)]
                if let Some((_, a2)) = &self.vec[*entry.get()] {
                    if a != *a2 {
                        panic!(
                            "comemo: found differing return values. \
                             is there an impure tracked function?"
                        )
                    }
                }
            }
        }
    }

    pub fn extract(&mut self, q: &Q) -> Option<A> {
        let h = crate::constraint::hash(&q);
        let i = *self.map.get(&h)?;
        self.vec[i].take().map(|(_, a)| a)
    }
}

impl<Q, A> Default for LookaheadSequence<Q, A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Q: Hash, A: Hash + Eq> FromIterator<(Q, A)> for LookaheadSequence<Q, A> {
    fn from_iter<T: IntoIterator<Item = (Q, A)>>(iter: T) -> Self {
        let mut seq = LookaheadSequence::new();
        for (q, a) in iter {
            seq.insert(q, a);
        }
        seq
    }
}

/// A tree data structure that associates a value with a sequence of (question,
/// answer) pairs.
///
/// Given an oracle, allows to efficiently query for a value for which every
/// question in the sequence yielded the same answer as the oracle will give for
/// that question.
pub struct QuestionTree<Q, A, T> {
    questions: Slab<Q>,
    results: Slab<T>,
    links: HashMap<(usize, A), State>,
    start: Option<State>,
}

impl<Q, A, T> QuestionTree<Q, A, T> {
    pub fn new() -> Self {
        Self {
            questions: Slab::new(),
            results: Slab::new(),
            links: HashMap::new(),
            start: None,
        }
    }
}

impl<Q, A, T> QuestionTree<Q, A, T>
where
    Q: Hash + Clone,
    A: Hash + Eq + Clone,
{
    pub fn get(&self, mut oracle: impl FnMut(&Q) -> A) -> Option<&T> {
        let mut state = self.start?;
        loop {
            match state.kind() {
                StateKind::Result(r) => return Some(self.results.get(r).unwrap()),
                StateKind::Question(qi) => {
                    let q = self.questions.get(qi).unwrap();
                    let a = oracle(q);
                    state = *self.links.get(&(qi, a))?;
                }
            }
        }
    }

    pub fn insert(
        &mut self,
        mut sequence: LookaheadSequence<Q, A>,
        value: T,
    ) -> Result<(), InsertError> {
        let mut state = self.start;
        let mut predecessor = None;

        loop {
            let pair = if state.is_none() || predecessor.is_some() {
                let Some((q, a)) = sequence.next() else { break };
                let qi = self.questions.alloc(q);
                let new = State::question(qi);
                self.link(predecessor.take(), new);
                state = Some(new);
                (qi, a)
            } else {
                let StateKind::Question(eqi) = state.unwrap().kind() else {
                    return Err(InsertError::AlreadyExists);
                };
                let eq = self.questions.get(eqi).unwrap();
                let Some(a) = sequence.extract(eq) else {
                    return Err(InsertError::WrongQuestion);
                };
                (eqi, a)
            };

            if let Some(&next) = self.links.get(&pair) {
                state = Some(next);
            } else {
                predecessor = Some(pair);
            }
        }

        if predecessor.is_none() && self.start.is_some() {
            return Err(InsertError::AlreadyExists);
        }

        let ri = self.results.alloc(value);
        self.link(predecessor, State::result(ri));

        Ok(())
    }

    fn link(&mut self, from: Option<(usize, A)>, target: State) {
        if self.start.is_none() {
            self.start = Some(target);
        }

        if let Some(pair) = from {
            self.links.insert(pair, target);
        }
    }
}

impl<Q: Debug, A: Debug, T: Debug> Debug for QuestionTree<Q, A, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (&(qi, ref a), next) in &self.links {
            let q = self.questions.get(qi).unwrap();
            write!(f, "[{qi}] ({q:?}, {a:?}) -> ")?;
            match next.kind() {
                StateKind::Question(qi) => writeln!(f, "{qi}")?,
                StateKind::Result(r) => {
                    writeln!(f, "{:?}", self.results.get(r).unwrap())?
                }
            }
        }
        Ok(())
    }
}

impl<Q, A, T> Default for QuestionTree<Q, A, T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum InsertError {
    AlreadyExists,
    WrongQuestion,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct State(isize);

impl State {
    fn question(i: usize) -> Self {
        Self(i as isize)
    }

    fn result(i: usize) -> Self {
        Self(-(i as isize) - 1)
    }

    fn kind(self) -> StateKind {
        if self.0 < 0 {
            StateKind::Result((-self.0) as usize - 1)
        } else {
            StateKind::Question(self.0 as usize)
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum StateKind {
    Question(usize),
    Result(usize),
}

struct Slab<T>(Vec<T>);

impl<T> Slab<T> {
    fn new() -> Self {
        Self(Vec::new())
    }

    fn alloc(&mut self, value: T) -> usize {
        let i = self.0.len();
        self.0.push(value);
        i
    }

    fn get(&self, i: usize) -> Option<&T> {
        self.0.get(i)
    }
}

impl<T> Default for Slab<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s<Q: Hash, A: Hash + Eq>(
        iter: impl IntoIterator<Item = (Q, A)>,
    ) -> LookaheadSequence<Q, A> {
        iter.into_iter().collect()
    }

    #[test]
    fn test_question_tree() {
        let mut tree = QuestionTree::<char, u128, &'static str>::new();
        tree.insert(s([('a', 10), ('b', 15)]), "first").unwrap();
        tree.insert(s([('a', 10), ('b', 20)]), "second").unwrap();
        tree.insert(s([('a', 15), ('c', 15)]), "third").unwrap();
        assert_eq!(
            tree.get(|&c| match c {
                'a' => 10,
                'b' => 15,
                _ => 20,
            }),
            Some(&"first")
        );
        assert_eq!(
            tree.get(|&c| match c {
                'a' => 10,
                _ => 20,
            }),
            Some(&"second")
        );
        assert_eq!(tree.get(|_| 15), Some(&"third"));
        assert_eq!(tree.get(|_| 10), None);
    }

    #[test]
    fn test_question_tree_pull_forward() {
        let mut tree = QuestionTree::<char, u128, &'static str>::new();
        tree.insert(s([('a', 10), ('b', 15)]), "first").unwrap();
        tree.insert(s([('a', 10), ('c', 15), ('b', 20)]), "second").unwrap();
        tree.insert(s([('a', 15), ('b', 30), ('c', 15)]), "third").unwrap();
        assert_eq!(
            tree.get(|&c| match c {
                'a' => 10,
                'b' => 20,
                'c' => 15,
                _ => 0,
            }),
            Some(&"second")
        );
        assert_eq!(
            tree.get(|&c| match c {
                'a' => 15,
                'b' => 30,
                'c' => 15,
                _ => 0,
            }),
            Some(&"third")
        );
    }

    #[test]
    fn test_cases_manual() {
        test_cases(vec![(vec![1, 0], 17)]);
        test_cases(vec![(vec![0], 0), (vec![1], 0)])
    }

    #[quickcheck_macros::quickcheck]
    fn test_cases_quickcheck(cases: Vec<(Vec<u16>, u8)>) {
        test_cases(cases);
    }

    fn test_cases(cases: Vec<(Vec<u16>, u8)>) {
        let mut tree = QuestionTree::new();
        let mut kept = Vec::new();
        for case in cases.iter() {
            let &(ref numbers, value) = case;
            match tree.insert(s(sequence(numbers)), value) {
                Ok(()) => kept.push(case),
                Err(InsertError::AlreadyExists) => {}
                Err(InsertError::WrongQuestion) => {} // Err(error) => panic!("{error:?}"),
            }
        }
        for (numbers, value) in kept {
            let map: HashMap<u64, u16> = sequence(numbers).collect();
            assert_eq!(tree.get(|s| map[s]), Some(value));
        }
    }

    fn sequence(numbers: &[u16]) -> impl Iterator<Item = (u64, u16)> {
        let mut state = 50;
        numbers.iter().map(move |&v| {
            let pair = (state, v);
            state += 1 + v as u64;
            pair
        })
    }
}
