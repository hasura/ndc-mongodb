use std::collections::BTreeMap;

use itertools::Itertools as _;
use ndc_models::{self as ndc, MutationRequest};

use crate::{self as plan, type_annotated_nested_field, MutationPlan};

use super::{
    plan_for_arguments::plan_for_mutation_procedure_arguments, query_plan_error::QueryPlanError,
    query_plan_state::QueryPlanState, QueryContext,
};

type Result<T> = std::result::Result<T, QueryPlanError>;

pub fn plan_for_mutation_request<T: QueryContext>(
    context: &T,
    request: MutationRequest,
) -> Result<MutationPlan<T>> {
    let operations = request
        .operations
        .into_iter()
        .map(|op| plan_for_mutation_operation(context, &request.collection_relationships, op))
        .try_collect()?;

    Ok(MutationPlan { operations })
}

fn plan_for_mutation_operation<T: QueryContext>(
    context: &T,
    collection_relationships: &BTreeMap<ndc::RelationshipName, ndc::Relationship>,
    operation: ndc::MutationOperation,
) -> Result<plan::MutationOperation<T>> {
    match operation {
        ndc::MutationOperation::Procedure {
            name,
            arguments,
            fields,
        } => {
            let mut plan_state = QueryPlanState::new(context, collection_relationships);

            let procedure_info = context.find_procedure(&name)?;

            let arguments = plan_for_mutation_procedure_arguments(
                &mut plan_state,
                &procedure_info.arguments,
                arguments,
            )?;

            let fields = fields
                .map(|nested_field| {
                    let result_type = context.ndc_to_plan_type(&procedure_info.result_type)?;
                    let plan_nested_field = type_annotated_nested_field(
                        context,
                        collection_relationships,
                        &result_type,
                        nested_field,
                    )?;
                    Ok(plan_nested_field) as Result<_>
                })
                .transpose()?;

            let relationships = plan_state.into_relationships();

            Ok(plan::MutationOperation::Procedure {
                name,
                arguments,
                fields,
                relationships,
            })
        }
    }
}
