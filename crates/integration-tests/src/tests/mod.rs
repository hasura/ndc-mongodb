// You might be getting an error message here from rust-analyzer:
//
// > file is not included module hierarchy
//
// To fix that update your editor LSP configuration with this setting:
//
//     rust-analyzer.cargo.allFeatures = true
//

mod aggregation;
mod basic;
mod expressions;
mod filtering;
mod local_relationship;
mod native_mutation;
mod native_query;
mod nested_collection;
mod permissions;
mod remote_relationship;
mod sorting;
