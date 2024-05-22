use ndc_models::Type;

pub fn named_type(name: impl ToString) -> Type {
    Type::Named {
        name: name.to_string(),
    }
}

pub fn nullable(t: impl Into<Type>) -> Type {
    Type::Nullable {
        underlying_type: Box::new(t.into()),
    }
}
