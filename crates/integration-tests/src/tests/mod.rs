// You might be getting an error message here from rust-analyzer:
//
// > file is not included module hierarchy
//
// To fix that update your editor LSP configuration with this setting:
//
//     rust-analyzer.cargo.allFeatures = true
//

mod basic;
mod native_procedure;
mod native_query;
mod remote_relationship;
