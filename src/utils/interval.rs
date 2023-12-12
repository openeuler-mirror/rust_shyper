use alloc::vec::{Vec, IntoIter};
use core::mem::MaybeUninit;

#[derive(Debug, Clone, Default)]
/// A struct to store all the discontinuous intervals.
pub struct IntervalExcluder {
    inner: Vec<(usize, isize)>,
}

#[derive(Debug, Clone)]
pub struct IntervalIter {
    inner: IntoIter<(usize, isize)>,
    sum: isize,
}

const INF: isize = u16::MAX as isize;

impl IntervalExcluder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a range of memory(give left and right) to the interval excluder.
    pub fn add_range(&mut self, left: usize, right: usize) -> &mut Self {
        self.inner.push((left, 1));
        self.inner.push((right, -1));
        if self.inner.len() > INF as usize {
            panic!("too many intervals");
        }
        self
    }

    /// Add a range of memory(give start and len) to the interval excluder.
    pub fn add_len(&mut self, start: usize, length: usize) -> &mut Self {
        self.add_range(start, start + length)
    }

    /// Exclude a range of memory(give left and right) from the interval excluder.
    pub fn exclude_range(&mut self, left: usize, right: usize) -> &mut Self {
        self.inner.push((left, -INF));
        self.inner.push((right, INF));
        self
    }

    /// Exclude a range of memory(give start and len) from the interval excluder.
    pub fn exclude_len(&mut self, start: usize, length: usize) -> &mut Self {
        self.exclude_range(start, start + length)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Interval {
    pub left: usize,
    pub right: usize,
}

impl Interval {
    pub fn len(&self) -> usize {
        self.right - self.left
    }
}

impl IntoIterator for IntervalExcluder {
    type Item = Interval;
    type IntoIter = IntervalIter;

    fn into_iter(self) -> Self::IntoIter {
        let mut inner = self.inner;
        inner.sort_unstable();
        let inner = inner
            .group_by(|a, b| a.0 == b.0)
            .map(|a| (a[0].0, a.iter().map(|t| t.1).sum()))
            .collect::<Vec<_>>()
            .into_iter();
        IntervalIter { inner, sum: 0 }
    }
}

/// An iterator over the intervals in an `IntervalExcluder`.
impl Iterator for IntervalIter {
    type Item = Interval;

    fn next(&mut self) -> Option<Self::Item> {
        let mut last = MaybeUninit::<usize>::uninit();
        for (pos, delta) in self.inner.by_ref() {
            let old = self.sum;
            self.sum += delta;
            if old <= 0 && self.sum > 0 {
                last.write(pos);
            } else if old > 0 && self.sum <= 0 {
                // SAFETY: `old > 0` implies `last` was initialized in the branch above
                let left = unsafe { last.assume_init() };
                return Some(Interval { left, right: pos });
            }
        }
        None
    }
}
