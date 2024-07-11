use ndc_models::Type;

pub fn array_of(t: impl Into<Type>) -> Type {
    Type::Array {
        element_type: Box::new(t.into()),
    }
}

pub fn named_type(name: impl ToString) -> Type {
    Type::Named {
        name: name.to_string().into(),
    }
}

pub fn nullable(t: impl Into<Type>) -> Type {
    Type::Nullable {
        underlying_type: Box::new(t.into()),
    }
}
