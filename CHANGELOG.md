# MongoDB Connector Changelog
This changelog documents the changes between release versions.

## [Unreleased]

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
- Initial release
