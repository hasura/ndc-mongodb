//! Column mapping between relational column indices and MongoDB field names.

use smol_str::SmolStr;

/// Tracks the mapping between column indices and field names
/// as the pipeline transforms data through stages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ColumnMapping {
    /// Ordered list of field names corresponding to column indices.
    columns: Vec<SmolStr>,
}

impl ColumnMapping {
    /// Create a new column mapping from a list of field names.
    pub fn new(columns: impl IntoIterator<Item = impl Into<SmolStr>>) -> Self {
        Self {
            columns: columns.into_iter().map(Into::into).collect(),
        }
    }

    /// Get the field name for a column index.
    ///
    /// Returns `None` if the index is out of bounds.
    pub fn field_for_index(&self, index: u64) -> Option<&str> {
        self.columns.get(index as usize).map(|s| s.as_str())
    }

    /// Get the number of columns in the mapping.
    #[must_use]
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// Check if the mapping is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// Iterate over all field names in order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.columns.iter().map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_mapping_from_field_names() {
        let mapping = ColumnMapping::new(["name", "age", "email"]);
        assert_eq!(mapping.len(), 3);
        assert!(!mapping.is_empty());
    }

    #[test]
    fn retrieves_field_by_index() {
        let mapping = ColumnMapping::new(["name", "age", "email"]);
        assert_eq!(mapping.field_for_index(0), Some("name"));
        assert_eq!(mapping.field_for_index(1), Some("age"));
        assert_eq!(mapping.field_for_index(2), Some("email"));
    }

    #[test]
    fn returns_none_for_out_of_bounds_index() {
        let mapping = ColumnMapping::new(["name", "age"]);
        assert_eq!(mapping.field_for_index(5), None);
        assert_eq!(mapping.field_for_index(100), None);
    }

    #[test]
    fn handles_empty_mapping() {
        let mapping = ColumnMapping::default();
        assert_eq!(mapping.len(), 0);
        assert!(mapping.is_empty());
        assert_eq!(mapping.field_for_index(0), None);
    }

    #[test]
    fn iterates_over_field_names() {
        let mapping = ColumnMapping::new(["a", "b", "c"]);
        let fields: Vec<_> = mapping.iter().collect();
        assert_eq!(fields, vec!["a", "b", "c"]);
    }
}
