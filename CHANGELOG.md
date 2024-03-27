# MongoDB Connector Changelog
This changelog documents the changes between release versions.

## [Unreleased]
- Use separate schema files for each collection
- Changes to `update` CLI command:
  - new default behaviour:
    - attempt to use validator schema if available
    - if no validator schema then sample documents from the collection
  - don't sample from collections that already have a schema
  - if no --sample-size given on command line, default sample size is 10
  - new option --no-validator-schema to disable attempting to use validator schema

## [0.0.2] - 2024-03-26
- Rename CLI plugin to ndc-mongodb ([PR #13](https://github.com/hasura/ndc-mongodb/pull/13))

## [0.0.1] - 2024-03-22
Initial release