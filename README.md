# Hasura MongoDB Data Connector

`![Optional Logo Image](path-to-image)`

[![Docs](https://img.shields.io/badge/docs-v3.x-brightgreen.svg?style=flat)](https://hasura.io/docs/3.0/connectors/mongodb/)
[![ndc-hub](https://img.shields.io/badge/ndc--hub-postgres-blue.svg?style=flat)](https://hasura.io/connectors/mongodb)
[![License](https://img.shields.io/badge/license-Apache--2.0-purple.svg?style=flat)](LICENSE.txt)

[Short description of the connector and its primary purpose.]
This Hasura Data Connector connects you to your SuperBlazingFastDB™ database. 

This connector is built using the [Rust Data Connector SDK](https://github.com/hasura/ndc-hub#rusk-sdk) and implements the [Data Connector Spec](https://github.com/hasura/ndc-spec).

- [See the listing in the Hasura Hub](https://hasura.io/connectors/[connector-name])
- [Hasura V3 Documentation](https://hasura.io/docs/3.0/)

Docs for the [connectorName] data connector:
<!-- TODO: Edit, change, delete as your wish. 
You must have Code of Conduct, Contributing, Support and Security though -->
- [Usage](./docs/usage/index.md)
  - [Querying (for example)](./docs/usage/querying.md)
- [Architecture](./docs/architecture.md)
- [Development](./docs/development.md)
- [Code of Conduct](./docs/code-of-conduct.md)
- [Contributing](./docs/contributing.md)
- [Authentication](./docs/authentication.md)
- [Debugging](./docs/debugging.md)
- [Limitations](./docs/limitations.md)
- [Production](./docs/production.md)
- [Support](./docs/support.md)
- [Security](./docs/security.md)
- [Edit and/or add additional relevant links]

## Features

Below, you'll find a matrix of all supported features for the [NAME] data connector:

<!-- TODO: Your README should contain only a single matrix; choose one below and remove either the ✅ or ❌ from each
row -->

<!-- OLTP matrix -->

| Feature                         | Supported | Notes |
| ------------------------------- | --------- | ----- |
| Native Queries + Logical Models | ✅ ❌     |       |
| Simple Object Query             | ✅ ❌     |       |
| Filter / Search                 | ✅ ❌     |       |
| Simple Aggregation              | ✅ ❌     |       |
| Sort                            | ✅ ❌     |       |
| Paginate                        | ✅ ❌     |       |
| Table Relationships             | ✅ ❌     |       |
| Views                           | ✅ ❌     |       |
| Remote Relationships            | ✅ ❌     |       |
| Mutations                       | ✅ ❌     |       |
| Distinct                        | ✅ ❌     |       |
| Enums                           | ✅ ❌     |       |
| Default Values                  | ✅ ❌     |       |
| User-defined Functions          | ✅ ❌     |       |

<!-- OLAP matrix -->
<!--
| Feature                         | Supported | Notes |
| ------------------------------- | --------- | ----- |
| Native Queries + Logical Models | ✅ ❌     |       |
| Simple Object Query             | ✅ ❌     |       |
| Filter / Search                 | ✅ ❌     |       |
| Simple Aggregation              | ✅ ❌     |       |
| Sort                            | ✅ ❌     |       |
| Paginate                        | ✅ ❌     |       |
| Table Relationships             | ✅ ❌     |       |
| Views                           | ✅ ❌     |       |
| Distinct                        | ✅ ❌     |       |
| Remote Relationships            | ✅ ❌     |       |
| Mutations                       | ✅ ❌     |       |
-->

<!-- DocDB matrix -->
<!--
| Feature                         | Supported | Notes |
| ------------------------------- | --------- | ----- |
| Native Queries + Logical Models | ✅ ❌     |       |
| Simple Object Query             | ✅ ❌     |       |
| Filter / Search                 | ✅ ❌     |       |
| Simple Aggregation              | ✅ ❌     |       |
| Sort                            | ✅ ❌     |       |
| Paginate                        | ✅ ❌     |       |
| Nested Objects                  | ✅ ❌     |       |
| Nested Arrays                   | ✅ ❌     |       |
| Nested Filtering                | ✅ ❌     |       |
| Nested Sorting                  | ✅ ❌     |       |
| Nested Relationships            | ✅ ❌     |       |
-->

## Before you get Started

[Prerequisites or recommended steps before using the connector.]

1. The [DDN CLI](https://hasura.io/docs/3.0/cli/installation) and [Docker](https://docs.docker.com/engine/install/) installed
2. A [supergraph](https://hasura.io/docs/3.0/getting-started/init-supergraph)
3. A [subgraph](https://hasura.io/docs/3.0/getting-started/init-subgraph)
<!-- TODO: add anything connector-specific here -->

The steps below explain how to Initialize and configure a connector for local development. You can learn how to deploy a
connector — after it's been configured — [here](https://hasura.io/docs/3.0/getting-started/deployment/deploy-a-connector).

## Using the [connectorName] connector

### Step 1: Authenticate your CLI session

```bash
ddn auth login
```

### Step 2: Configure the connector

Once you have an initialized supergraph and subgraph, run the initialization command in interactive mode while 
providing a name for the connector in the prompt:

```bash
ddn connector init <connector-name> -i
```

#### Step 2.1: Choose the [connectorName] from the list
<!-- Let your users know which name to choose from the list of connectors -->

#### Step 2.2: Choose a port for the connector

The CLI will ask for a specific port to run the connector on. Choose a port that is not already in use or use the 
default suggested port.

#### Step 2.3: Provide the [name] env var(s) for the connector

<!-- Instruct your users on how to provide the necessary environment variables that the connector requires. Include 
where to find them. Also, if applicable, provide default read-only connection credentials that a testing user can 
use without needing to set up an instance of the source. -->

| Name          | Description                                                                                                                                       |
|---------------|---------------------------------------------------------------------------------------------------------------------------------------------------|
| ConnectionURI | The connection string for the SuperBlazingFastDB™ database. You can find this on your [connection tab](http://www.example.com/) of your instance. |
| AnotherVar    | Etc...                                                                                                                                            |
  
## Step 3: Introspect the connector

<!-- Tell your users what config files this step will generate.-->

```bash
ddn connector introspect <connector-name>
```

## Step 4: Add your resources

<!-- Tell your users what metadata objects will be created from this command.-->

```bash
ddn connector-link add-resources <connector-name>
```

## Documentation
<!-- TODO: Either on GitHub in this repo or on your own site -->
View the full documentation for the [connectorName] connector [here](./docs/index.md).

## Contributing

Check out our [contributing guide](./docs/contributing.md) for more details.

## License

The [connectorName] connector is available under the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).

## Additional Sections

[Any other relevant sections or details specific to the connector.]
