use serde::{Deserialize, Serialize};

/// Helper for working with serialized formats of named values. This is for cases where we want to
/// deserialize to a map where names are stored as map keys. But in serialized form the name may be
/// an inline field.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct WithName<T> {
    pub name: String,
    #[serde(flatten)]
    pub value: T,
}

impl<T> WithName<T> {
    pub fn into_map<Map>(values: impl IntoIterator<Item = WithName<T>>) -> Map
    where
        Map: FromIterator<(String, T)>,
    {
        values
            .into_iter()
            .map(Self::into_name_value_pair)
            .collect::<Map>()
    }

    pub fn into_name_value_pair(self) -> (String, T) {
        (self.name, self.value)
    }

    pub fn named(name: impl ToString, value: T) -> Self {
        WithName {
            name: name.to_string(),
            value,
        }
    }

    pub fn as_ref<R>(&self) -> WithNameRef<'_, R>
    where
        T: AsRef<R>,
    {
        WithNameRef::named(&self.name, self.value.as_ref())
    }
}

impl<T> From<WithName<T>> for (String, T) {
    fn from(value: WithName<T>) -> Self {
        value.into_name_value_pair()
    }
}

impl<T> From<(String, T)> for WithName<T> {
    fn from((name, value): (String, T)) -> Self {
        WithName::named(name, value)
    }
}

#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct WithNameRef<'a, T> {
    pub name: &'a str,
    pub value: &'a T,
}

impl<'a, T> WithNameRef<'a, T> {
    pub fn named<'b>(name: &'b str, value: &'b T) -> WithNameRef<'b, T> {
        WithNameRef { name, value }
    }

    pub fn to_owned<R>(&self) -> WithName<R>
    where
        T: ToOwned<Owned = R>,
    {
        WithName::named(self.name.to_owned(), self.value.to_owned())
    }
}

impl<'a, T, R> From<&'a WithName<T>> for WithNameRef<'a, R>
where
    T: AsRef<R>,
{
    fn from(value: &'a WithName<T>) -> Self {
        value.as_ref()
    }
}
