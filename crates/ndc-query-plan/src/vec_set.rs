/// Set implementation that only requires an [Eq] implementation on its value type
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VecSet<T> {
    items: Vec<T>,
}

impl<T> VecSet<T> {
    pub fn new() -> Self {
        VecSet { items: Vec::new() }
    }

    pub fn singleton(value: T) -> Self {
        VecSet { items: vec![value] }
    }

    /// If the value does not exist in the set, inserts it and returns `true`. If the value does
    /// exist returns `false`, and leaves the set unchanged.
    pub fn insert(&mut self, value: T) -> bool
    where
        T: Eq,
    {
        if self.items.iter().any(|v| *v == value) {
            false
        } else {
            self.items.push(value);
            true
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<T> FromIterator<T> for VecSet<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        VecSet {
            items: Vec::from_iter(iter),
        }
    }
}

impl<T, const N: usize> From<[T; N]> for VecSet<T> {
    fn from(value: [T; N]) -> Self {
        VecSet {
            items: value.into(),
        }
    }
}

impl<T> IntoIterator for VecSet<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a VecSet<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut VecSet<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter_mut()
    }
}
