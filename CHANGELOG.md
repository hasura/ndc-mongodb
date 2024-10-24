# MongoDB Connector Changelog

This changelog documents the changes between release versions.

## [Unreleased]

### Added

### Changed

- **BREAKING:** If `configuration.json` cannot be parsed the connector will fail to start. This change also prohibits unknown keys in that file. These changes will help to prevent typos configuration being silently ignored. ([#115](https://github.com/hasura/ndc-mongodb/pull/115))

### Fixed

- Fixes for filtering by complex predicate that references variables, or field names that require escaping ([#111](https://github.com/hasura/ndc-mongodb/pull/111))
- Escape names if necessary instead of failing when joining relationship on field names with special characters ([#113](https://github.com/hasura/ndc-mongodb/pull/113))

## [1.3.0] - 2024-10-01

### Fixed

- Selecting nested fields with names that begin with a dollar sign ([#108](https://github.com/hasura/ndc-mongodb/pull/108))
- Sorting by fields with names that begin with a dollar sign ([#109](https://github.com/hasura/ndc-mongodb/pull/109))

### Changed

## [1.2.0] - 2024-09-12

### Added

- Extended JSON fields now support all comparison and aggregation functions ([#99](https://github.com/hasura/ndc-mongodb/pull/99))
- Update to ndc-spec v0.1.6 which allows filtering by object values in array fields ([#101](https://github.com/hasura/ndc-mongodb/pull/101))

#### Filtering by values in arrays

In this update you can filter by making comparisons to object values inside
arrays. For example consider a MongoDB database with these three documents:

```json
{ "institution": "Black Mesa", "staff": [{ "name": "Freeman" }, { "name": "Calhoun" }] }
{ "institution": "Aperture Science", "staff": [{ "name": "GLaDOS" }, { "name": "Chell" }] }
{ "institution": "City 17", "staff": [{ "name": "Alyx" }, { "name": "Freeman" }, { "name": "Breen" }] }
```

You can now write a GraphQL query with a `where` clause that checks individual
entries in the `staff` arrays:

```graphql
query {
  institutions(where: { staff: { name: { _eq: "Freeman" } } }) {
    institution
  }
}
```

Which produces the result:

```json
{ "data": { "institutions": [
  { "institution": "Black Mesa" },
  { "institution": "City 17" } 
] } }
```

The filter selects documents where **any** element in the array passes the
condition. If you want to select only documents where _every_ array element
passes then negate the comparison on array element values, and also negate the
entire predicate like this:

```graphql
query EveryElementMustMatch {
  institutions(
    where: { _not: { staff: { name: { _neq: "Freeman" } } } }
  ) {
    institution
  }
}
```

**Note:** It is currently only possible to filter on arrays that contain
objects. Filtering on arrays that contain scalar values or nested arrays will
come later.

To configure DDN metadata to filter on array fields configure the
`BooleanExpressionType` for the containing document object type to use an
**object** boolean expression type for comparisons on the array field. The
GraphQL Engine will transparently distribute object comparisons over array
elements. For example the above example is configured with this boolean
expression type for documents:

```yaml
---
kind: BooleanExpressionType
version: v1
definition:
  name: InstitutionComparisonExp
  operand:
    object:
      type: Institution
      comparableFields:
        - fieldName: id
          booleanExpressionType: ObjectIdComparisonExp
        - fieldName: institution
          booleanExpressionType: StringComparisonExp
        - fieldName: staff
          booleanExpressionType: InstitutionStaffComparisonExp
      comparableRelationships: []
  logicalOperators:
    enable: true
  isNull:
    enable: true
  graphql:
    typeName: InstitutionComparisonExp
```

`InstitutionStaffComparisonExp` is the boolean expression type for objects
inside the `staff` array. It looks like this:

```yaml
---
kind: BooleanExpressionType
version: v1
definition:
  name: InstitutionStaffComparisonExp
  operand:
    object:
      type: InstitutionStaff
      comparableFields:
        - fieldName: name
          booleanExpressionType: StringComparisonExp
      comparableRelationships: []
  logicalOperators:
    enable: true
  isNull:
    enable: true
  graphql:
    typeName: InstitutionStaffComparisonExp
```

## [1.1.0] - 2024-08-16

- Accept predicate arguments in native mutations and native queries ([#92](https://github.com/hasura/ndc-mongodb/pull/92))
- Serialize aggregate results as simple JSON (instead of Extended JSON) for
  consistency with non-aggregate result serialization ([#96](https://github.com/hasura/ndc-mongodb/pull/96))

## [1.0.0] - 2024-07-09

- Fix bug with operator lookup when filtering on nested fields ([#82](https://github.com/hasura/ndc-mongodb/pull/82))
- Rework query plans for requests with variable sets to allow use of indexes ([#83](https://github.com/hasura/ndc-mongodb/pull/83))
- Fix: error when requesting query plan if MongoDB is target of a remote join ([#83](https://github.com/hasura/ndc-mongodb/pull/83))
- Fix: count aggregates return 0 instead of null if no rows match ([#85](https://github.com/hasura/ndc-mongodb/pull/85))
- Breaking change: remote joins no longer work in MongoDB v5 ([#83](https://github.com/hasura/ndc-mongodb/pull/83))
- Add configuration option to opt into "relaxed" mode for Extended JSON outputs ([#84](https://github.com/hasura/ndc-mongodb/pull/84))

## [0.1.0] - 2024-06-13

- Support filtering and sorting by fields of related collections ([#72](https://github.com/hasura/ndc-mongodb/pull/72))
- Support for root collection column references ([#75](https://github.com/hasura/ndc-mongodb/pull/75))
- Fix for databases with field names that begin with a dollar sign, or that contain dots ([#74](https://github.com/hasura/ndc-mongodb/pull/74))
- Implement column-to-column comparisons within the same collection ([#74](https://github.com/hasura/ndc-mongodb/pull/74))
- Fix error tracking collection with no documents by skipping such collections during CLI introspection ([#76](https://github.com/hasura/ndc-mongodb/pull/76))
- If a field contains both `int` and `double` values then the field type is inferred as `double` instead of `ExtendedJSON` ([#77](https://github.com/hasura/ndc-mongodb/pull/77))
- Fix: schema generated with `_id` column nullable when introspecting schema via sampling ([#78](https://github.com/hasura/ndc-mongodb/pull/78))
- Don't require _id field to have type ObjectId when generating primary uniqueness constraint ([#79](https://github.com/hasura/ndc-mongodb/pull/79))

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
- Update `rustls` dependency to fix <https://github.com/hasura/ndc-mongodb/security/dependabot/1> ([#51](https://github.com/hasura/ndc-mongodb/pull/51))
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
