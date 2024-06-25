/// Is this the top-level query in a request, or is it a query for a relationship?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryLevel {
    Top,
    Relationship,
}
