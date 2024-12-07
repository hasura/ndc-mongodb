use serde::{Deserialize, Serialize};

/// Helper for working with serialized formats of named values. This is for cases where we want to
/// deserialize to a map where names are stored as map keys. But in serialized form the name may be
/// an inline field.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct WithName<N, T> {
    pub name: N,
    #[serde(flatten)]
    pub value: T,
}

impl<N, T> WithName<N, T> {
    pub fn into_map<Map>(values: impl IntoIterator<Item = WithName<N, T>>) -> Map
    where
        Map: FromIterator<(N, T)>,
    {
        values
            .into_iter()
            .map(Self::into_name_value_pair)
            .collect::<Map>()
    }

    pub fn into_name_value_pair(self) -> (N, T) {
        (self.name, self.value)
    }

    pub fn named(name: N, value: T) -> Self {
        WithName { name, value }
    }

    pub fn as_ref<RN, RT>(&self) -> WithNameRef<'_, RN, RT>
    where
        N: AsRef<RN>,
        T: AsRef<RT>,
    {
        WithNameRef::named(self.name.as_ref(), self.value.as_ref())
    }
}

impl<N, T> From<WithName<N, T>> for (N, T) {
    fn from(value: WithName<N, T>) -> Self {
        value.into_name_value_pair()
    }
}

impl<N, T> From<(N, T)> for WithName<N, T> {
    fn from((name, value): (N, T)) -> Self {
        WithName::named(name, value)
    }
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct WithNameRef<'a, N, T> {
    pub name: &'a N,
    pub value: &'a T,
}

impl<N, T> WithNameRef<'_, N, T> {
    pub fn named<'b>(name: &'b N, value: &'b T) -> WithNameRef<'b, N, T> {
        WithNameRef { name, value }
    }

    pub fn to_owned<RN, RT>(&self) -> WithName<RN, RT>
    where
        N: ToOwned<Owned = RN>,
        T: ToOwned<Owned = RT>,
    {
        WithName::named(self.name.to_owned(), self.value.to_owned())
    }
}

impl<'a, N, T, RN, RT> From<&'a WithName<N, T>> for WithNameRef<'a, RN, RT>
where
    N: AsRef<RN>,
    T: AsRef<RT>,
{
    fn from(value: &'a WithName<N, T>) -> Self {
        value.as_ref()
    }
}
