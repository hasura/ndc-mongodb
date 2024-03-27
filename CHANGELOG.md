# MongoDB Connector Changelog
This changelog documents the changes between release versions.

## [Unreleased]
- Use separate schema files for each collection
- Don't sample from collections that already have a schema
- New default behaviour for `update` CLI command: for each collection
  - attempt to use validator schema if available
  - if no validator schema, sample documents
  - default sample size is 10

## [0.0.2] - 2024-03-26
- Rename CLI plugin to ndc-mongodb ([PR #13](https://github.com/hasura/ndc-mongodb/pull/13))

## [0.0.1] - 2024-03-22
Initial release