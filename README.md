# Hasura MongoDB Data Connector

[![Docs](https://img.shields.io/badge/docs-v3.x-brightgreen.svg?style=flat)](https://hasura.io/docs/3.0/connectors/mongodb/)
[![ndc-hub](https://img.shields.io/badge/ndc--hub-postgres-blue.svg?style=flat)](https://hasura.io/connectors/mongodb)
[![License](https://img.shields.io/badge/license-Apache--2.0-purple.svg?style=flat)](LICENSE.txt)

This Hasura data connector connects MongoDB to your data graph giving you an
instant GraphQL API to access your MongoDB data. Supports MongoDB 6 or later.

This connector is built using the [Rust Data Connector SDK](https://github.com/hasura/ndc-hub#rusk-sdk) and implements the [Data Connector Spec](https://github.com/hasura/ndc-spec).

- [See the listing in the Hasura Hub](https://hasura.io/connectors/mongodb)
- [Hasura V3 Documentation](https://hasura.io/docs/3.0/)

Docs for the MongoDB data connector:

- [Usage](https://hasura.io/docs/3.0/connectors/mongodb/)
- [Building](./docs/building.md)
- [Development](./docs/development.md)
- [Docker Images](./docs/docker-images.md)
- [Code of Conduct](./docs/code-of-conduct.md)
- [Contributing](./docs/contributing.md)
- [Limitations](./docs/limitations.md)
- [Support](./docs/support.md)
- [Security](./docs/security.md)

## Features

Below, you'll find a matrix of all supported features for the MongoDB data connector:

| Feature                                         | Supported | Notes |
| ----------------------------------------------- | --------- | ----- |
| Native Queries + Logical Models                 | ✅        |       |
| Simple Object Query                             | ✅        |       |
| Filter / Search                                 | ✅        |       |
| Filter by fields of Nested Objects              | ✅        |       |
| Filter by values in Nested Arrays               | ✅        |       |
| Simple Aggregation                              | ✅        |       |
| Aggregate fields of Nested Objects              | ❌        |       |
| Aggregate values of Nested Arrays               | ❌        |       |
| Sort                                            | ✅        |       |
| Sorty by fields of Nested Objects               | ❌        |       |
| Paginate                                        | ✅        |       |
| Collection Relationships                        | ✅        |       |
| Remote Relationships                            | ✅        |       |
| Relationships Keyed by Fields of Nested Objects | ❌        |       |
| Mutations                                       | ✅        | Provided by custom [Native Mutations](TODO) - predefined basic mutations are also planned |

## Before you get Started

1. The [DDN CLI](https://hasura.io/docs/3.0/cli/installation) and [Docker](https://docs.docker.com/engine/install/) installed
2. A [supergraph](https://hasura.io/docs/3.0/getting-started/init-supergraph)
3. A [subgraph](https://hasura.io/docs/3.0/getting-started/init-subgraph)

The steps below explain how to initialize and configure a connector for local
development on your data graph. You can learn how to deploy a connector — after
it's been configured
— [here](https://hasura.io/docs/3.0/getting-started/deployment/deploy-a-connector).

For instructions on local development on the MongoDB connector itself see
[development.md](development.md).

## Using the MongoDB connector

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

`<connector-name>` may be any name you choose for your particular project.

#### Step 2.1: Choose the hasura/mongodb from the list

#### Step 2.2: Choose a port for the connector

The CLI will ask for a specific port to run the connector on. Choose a port that is not already in use or use the 
default suggested port.

#### Step 2.3: Provide env vars for the connector

| Name                   | Description                                                          |
|------------------------|----------------------------------------------------------------------|
| `MONGODB_DATABASE_URI` | Connection URI for the MongoDB database to connect - see notes below |

`MONGODB_DATABASE_URI` is a string with your database' hostname, login
credentials, and database name. A simple example is
`mongodb://admin@pass:localhost/my_database`. If you are using a hosted database
on MongoDB Atlas you can get the URI from the "Data Services" tab in the project
dashboard:

- open the "Data Services" tab
- click "Get connection string"
- you will see a 3-step dialog - ignore all 3 steps, you don't need to change anything
- copy the string that begins with `mongodb+srv://`
  
## Step 3: Introspect the connector

Set up configuration for the connector with this command. This will introspect
your database to infer a schema with types for your data.

```bash
ddn connector introspect <connector-name>
```

Remember to use the same value for `<connector-name>` That you used in step 2.

This will create a tree of files that looks like this (this example is based on the
[sample_mflix][] sample database):

[sample_mflix]: https://www.mongodb.com/docs/atlas/sample-data/sample-mflix/

```
app/connector
└── <connector-name>
   ├── compose.yaml        -- defines a docker service for the connector
   ├── connector.yaml      -- defines connector version to fetch from hub, subgraph, env var mapping
   ├── configuration.json  -- options for configuring the connector
   ├── schema              -- inferred types for collection documents - one file per collection
   │  ├── comments.json
   │  ├── movies.json
   │  ├── sessions.json
   │  ├── theaters.json
   │  └── users.json
   ├── native_mutations    -- custom mongodb commands to appear in your data graph
   │  └── your_mutation.json
   └── native_queries      -- custom mongodb aggregation pipelines to appear in your data graph
      └── your_query.json
```

The `native_mutations` and `native_queries` directories will not be created
automatically - create those directories as needed.

Feel free to edit these files to change options, or to make manual tweaks to
inferred schema types. If inferred types do not look accurate you can edit
`configuration.json`, change `sampleSize` to a larger number to randomly sample
more collection documents, and run the `introspect` command again.

## Step 4: Add your resources

This command will query the MongoDB connector to produce DDN metadata that
declares resources provided by the connector in your data graph.

```bash
ddn connector-link add-resources <connector-name>
```

The connector must be running before you run this command! If you have not
already done so you can run the connector with `ddn run docker-start`.

If you have changed the configuration described in Step 3 it is important to
restart the connector. Running `ddn run docker-start` again will restart the
connector if configuration has changed.

This will create and update DDN metadata files. Once again this example is based
on the [sample_mflix][] data set:

```
app/metadata
├── mongodb.hml        -- DataConnectorLink has connector connection details & database schema
├── mongodb-types.hml  -- maps connector scalar types to GraphQL scalar types
├── Comments.hml       -- The remaining files map database collections to GraphQL object types
├── Movies.hml
├── Sessions.hml
├── Theaters.hml
└── Users.hml
```

## Documentation

View the full documentation for the MongoDB connector [here](https://hasura.io/docs/3.0/connectors/mongodb/).

## Contributing

Check out our [contributing guide](./docs/contributing.md) for more details.

## License

The MongoDB connector is available under the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).
