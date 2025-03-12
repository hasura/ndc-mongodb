use derivative::Derivative;
use ndc_models as ndc;

use crate::Type;

use super::ConnectorTypes;

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum ComparisonOperatorDefinition<T: ConnectorTypes> {
    Equal,
    In,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Contains,
    ContainsInsensitive,
    StartsWith,
    StartsWithInsensitive,
    EndsWith,
    EndsWithInsensitive,
    Custom {
        /// The type of the argument to this operator
        argument_type: Type<T::ScalarType>,
    },
}

impl<T: ConnectorTypes> ComparisonOperatorDefinition<T> {
    pub fn argument_type(self, left_operand_type: &Type<T::ScalarType>) -> Type<T::ScalarType> {
        use ComparisonOperatorDefinition as C;
        match self {
            C::In => Type::ArrayOf(Box::new(left_operand_type.clone())),
            C::Equal
            | C::LessThan
            | C::LessThanOrEqual
            | C::GreaterThan
            | C::GreaterThanOrEqual => left_operand_type.clone(),
            C::Contains
            | C::ContainsInsensitive
            | C::StartsWith
            | C::StartsWithInsensitive
            | C::EndsWith
            | C::EndsWithInsensitive => T::string_type(),
            C::Custom { argument_type } => argument_type,
        }
    }

    pub fn from_ndc_definition<E>(
        ndc_definition: &ndc::ComparisonOperatorDefinition,
        map_type: impl FnOnce(&ndc::Type) -> Result<Type<T::ScalarType>, E>,
    ) -> Result<Self, E> {
        use ndc::ComparisonOperatorDefinition as NDC;
        let definition = match ndc_definition {
            NDC::Equal => Self::Equal,
            NDC::In => Self::In,
            NDC::LessThan => Self::LessThan,
            NDC::LessThanOrEqual => Self::LessThanOrEqual,
            NDC::GreaterThan => Self::GreaterThan,
            NDC::GreaterThanOrEqual => Self::GreaterThanOrEqual,
            NDC::Contains => Self::Contains,
            NDC::ContainsInsensitive => Self::ContainsInsensitive,
            NDC::StartsWith => Self::StartsWith,
            NDC::StartsWithInsensitive => Self::StartsWithInsensitive,
            NDC::EndsWith => Self::EndsWith,
            NDC::EndsWithInsensitive => Self::EndsWithInsensitive,
            NDC::Custom { argument_type } => Self::Custom {
                argument_type: map_type(argument_type)?,
            },
        };
        Ok(definition)
    }
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct AggregateFunctionDefinition<T: ConnectorTypes> {
    /// The scalar or object type of the result of this function
    pub result_type: Type<T::ScalarType>,
}
