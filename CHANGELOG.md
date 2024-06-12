# MongoDB Connector Changelog
This changelog documents the changes between release versions.

## [Unreleased]
- Support filtering and sorting by fields of related collections ([#72](https://github.com/hasura/ndc-mongodb/pull/72))
- Support for root collection column references ([#75](https://github.com/hasura/ndc-mongodb/pull/75))
- Fix for databases with field names that begin with a dollar sign, or that contain dots ([#74](https://github.com/hasura/ndc-mongodb/pull/74))
- Implement column-to-column comparisons within the same collection ([#74](https://github.com/hasura/ndc-mongodb/pull/74))
- If a field contains both `int` and `double` values then the field type is inferred as `double` instead of `ExtendedJSON` ([#76](https://github.com/hasura/ndc-mongodb/pull/76))

## [0.0.6] - 2024-05-01
- Enables logging events from the MongoDB driver by setting the `RUST_LOG` variable ([#67](https://github.com/hasura/ndc-mongodb/pull/67))
  - To log all events set `RUST_LOG=mongodb::command=debug,mongodb::connection=debug,mongodb::server_selection=debug,mongodb::topology=debug`
- Relations with a single column mapping now use concise correlated subquery syntax in `$lookup` stage ([#65](https://github.com/hasura/ndc-mongodb/pull/65))
- Add root `configuration.json` or `configuration.yaml` file to allow editing cli options. ([#68](https://github.com/hasura/ndc-mongodb/pull/68))
- Update default sample size to 100. ([#68](https://github.com/hasura/ndc-mongodb/pull/68))
- Add `all_schema_nullable` option defaulted to true. ([#68](https://github.com/hasura/ndc-mongodb/pull/68))
- Change `native_procedure` to `native_mutation` along with code renaming ([#70](https://github.com/hasura/ndc-mongodb/pull/70))
  - Note: `native_procedures` folder in configuration is not deprecated. It will continue to work for a few releases, but renaming your folder is all that is needed.

## [0.0.5] - 2024-04-26
- Fix incorrect order of results for query requests with more than 10 variable sets (#37)
- In the CLI update command, don't overwrite schema files that haven't changed ([#49](https://github.com/hasura/ndc-mongodb/pull/49/files))
- In the CLI update command, if the database URI is not provided the error message now mentions the correct environment variable to use (`MONGODB_DATABASE_URI`) ([#50](https://github.com/hasura/ndc-mongodb/pull/50))
- Update to latest NDC SDK ([#51](https://github.com/hasura/ndc-mongodb/pull/51))
- Update `rustls` dependency to fix https://github.com/hasura/ndc-mongodb/security/dependabot/1 ([#51](https://github.com/hasura/ndc-mongodb/pull/51))
- Serialize query and mutation response fields with known types using simple JSON instead of Extended JSON (#53) (#59)
- Add trace spans ([#58](https://github.com/hasura/ndc-mongodb/pull/58))

## [0.0.4] - 2024-04-12
- Queries that attempt to compare a column to a column in the query root table, or a related table, will now fail instead of giving the incorrect result ([#22](https://github.com/hasura/ndc-mongodb/pull/22))
- Fix bug in v2 to v3 conversion of query responses containing nested objects ([PR #27](https://github.com/hasura/ndc-mongodb/pull/27))
- Fixed bug where use of aggregate functions in queries would fail ([#26](https://github.com/hasura/ndc-mongodb/pull/26))
- Rename Any type to ExtendedJSON to make its representation clearer ([#30](https://github.com/hasura/ndc-mongodb/pull/30))
- The collection primary key `_id` property now has a unique constraint generated in the NDC schema for it ([#32](https://github.com/hasura/ndc-mongodb/pull/32))

## [0.0.3] - 2024-03-28
- Use separate schema files for each collection ([PR #14](https://github.com/hasura/ndc-mongodb/pull/14))
- Changes to `update` CLI command ([PR #17](https://github.com/hasura/ndc-mongodb/pull/17)):
  - new default behaviour:
    - attempt to use validator schema if available
    - if no validator schema then sample documents from the collection
  - don't sample from collections that already have a schema
  - if no --sample-size given on command line, default sample size is 10
  - new option --no-validator-schema to disable attempting to use validator schema
- Add `any` type and use it to represent mismatched types in sample documents ([PR #18](https://github.com/hasura/ndc-mongodb/pull/18))

## [0.0.2] - 2024-03-26
- Rename CLI plugin to ndc-mongodb ([PR #13](https://github.com/hasura/ndc-mongodb/pull/13))

## [0.0.1] - 2024-03-22
Initial release
