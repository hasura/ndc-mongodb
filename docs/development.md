# MongoDB Data Connector Development

These are instructions for building and running the MongoDB Data Connector - and
supporting services - locally for purposes of working on the connector itself.

This repo is set up to run all necessary services for interactive and
integration testing in docker containers with pre-populated MongoDB databases
with just one command, `just up`, if you have the prerequisites installed.
Repeating that command restarts services as necessary to apply code or
configuration changes.

## Prerequisites

- [Nix][Determinate Systems Nix Installer]
- [Docker](https://docs.docker.com/engine/install/)
- [Just](https://just.systems/man/en/) (optional)

The easiest way to set up build and development dependencies for this project is
to use Nix. If you don't already have Nix we recommend the [Determinate Systems
Nix Installer][] which automatically applies settings required by this project.

[Determinate Systems Nix Installer]: https://github.com/DeterminateSystems/nix-installer/blob/main/README.md

You may optionally install `just`. If you are using a Nix develop shell it
provides `just` automatically. (See "The development shell" below).

If you prefer to manage dependencies yourself you will need,

* Rust via Rustup
* MongoDB `>= 6`
* OpenSSL development files

## Quickstart

To run everything you need run this command to start services in Docker
containers:

```sh
$ just up
```

Next access the GraphQL interface at http://localhost:7100/

Run the above command again to restart any services that are affected by code
changes or configuration changes.

## The development shell

This project uses a development shell configured in `flake.nix` that automatically
loads specific version of Rust along with all other project dependencies. The
development shell provides:

- a Rust toolchain: `cargo`, `cargo-clippy`, `rustc`, `rustfmt`, etc.
- `cargo-insta` for reviewing test snapshots
- `just`
- `mongosh`
- `arion` which is a Nix frontend for docker-compose
- The DDN CLI
- The MongoDB connector plugin for the DDN CLI which is automatically rebuilt after code changes in this repo (can be run directly with `mongodb-cli-plugin`)

Development shell features are specified in the `devShells` definition in
`flake.nix`. You can add dependencies by [looking up the Nix package
name](https://search.nixos.org/), and adding the package name to the
`nativeBuildInputs` list.

The simplest way to start a development shell is with this command:

```sh
$ nix develop
```

If you are going to be doing a lot of work on this project it can be more
convenient to set up [direnv][] which automatically links project dependencies
in your shell when you cd to the project directory, and automatically reverses
all shell modifications when you navigate to another directory. You can also set
up direnv integration in your editor to get your editor LSP to use the same
version of Rust that the project uses.

[direnv]: https://direnv.net/

## Running and Testing

There is a `justfile` for getting started quickly. You can use its recipes to
run relevant services locally including the MongoDB connector itself, a MongoDB
database server, and the Hasura GraphQL Engine. Use these commands:

```sh
just up           # start services; run this again to restart after making code changes
just down         # stop services
just down-volumes # stop services, and remove MongoDB database volume
just logs         # see service logs
just test         # run unit and integration tests
just              # list available recipes
```

Integration tests run in an independent set of ephemeral docker containers.

The `just` command is provided automatically if you are using the development
shell. Or you can install it yourself.

The typical workflow for interactive testing (testing by hand) is to interact
with the system through the Hasura GraphQL Engine's GraphQL UI at
http://localhost:7100/. If you can get insight into what the connector is doing
by reading the logs which you can access by running `just logs`, or via the
Jaeger UI at http://localhost:16686/.

### Running with a different MongoDB version

Override the MongoDB version by assigning a Docker image name to the environment
variable `MONGODB_IMAGE`. For example,

    $ just down-volumes # delete potentially-incompatible MongoDB data
    $ MONGODB_IMAGE=mongo:6 arion up -d

Or run integration tests against a specific MongoDB version,

    $ MONGODB_IMAGE=mongo:6 just test-integration

There is a predefined just recipe that runs integration tests using MongoDB
versions 5, 6, and 7. There is some functionality that does not work in MongoDB
v5 so some tests are skipped when running that MongoDB version.

### Where to find the tests

Unit tests are found in conditionally-compiled test modules in the same Rust
source code files with the code that the tests test.

Integration tests are found in `crates/integration-tests/src/tests/`

### Writing Integration Tests

Integration tests are run with `just test-integration`. Typically integration
tests run a GraphQL query, and compare the response to a saved snapshot. Here is
an example:

```rust
#[tokio::test]
async fn filters_by_date() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query ($dateInput: Date) {
                  movies(
                    order_by: {id: Asc},
                    where: {released: {_gt: $dateInput}}
                  ) {
                    title
                    released
                  }
                }
            "#
        )
        .variables(json!({ "dateInput": "2016-03-01T00:00Z" }))
        .run()
        .await?
    );
    Ok(())
}
```

On the first test run after a test is created or changed the test runner will
create a new snapshot file with the GraphQL response. To make the test pass it
is necessary to approve the snapshot (if the response is correct). To do that
run,

```sh
$ cargo insta review
```

Approved snapshot files must be checked into version control.

Please be aware that MongoDB query results do not have consistent ordering. It
is important to have `order_by` clauses in every test that produces more than
one result to explicitly order everything. Otherwise tests will fail when the
order of a response does not match the exact order of data in an approved
snapshot.

## Building

For instructions on building binaries or Docker images see [building.md](./building.md).

## Working with Test Data

### Predefined MongoDB databases

This repo includes fixture data and configuration to provide a fully-configured
data graph for testing.

There are three provided MongoDB databases. Development services run three
connector instances to provide access to each of those. Listing these by Docker
Compose service names:

- `connector` serves the [sample_mflix][] database
- `connector-chinook` serves a version of the [chinook][] sample database that has been adapted for MongoDB
- `connector-test-cases` serves the test_cases database - if you want to set up data for integration tests put it in this database

[sample_mflix]: https://www.mongodb.com/docs/atlas/sample-data/sample-mflix/
[chinook]: https://github.com/lerocha/chinook-database

Those databases are populated by scripts in `fixtures/mongodb/`. There is
a subdirectory with fixture data for each database.

Integration tests use an ephemeral MongoDB container so a fresh database will be
populated with those fixtures on every test run.

Interactive services (the ones you get with `just up`) use a persistent volume
for MongoDB databases. To get updated data after changing fixtures, or any time
you want to get a fresh database, you will have to delete the volume and
recreate the MongoDB container. To do that run,

```sh
$ just down-volumes
$ just up
```

### Connector Configuration

If you followed the Quickstart in [README.md](../README.md) then you got
connector configuration in your data graph project in
`app/connector/<connector-name>/`. This repo provides predefined connector
configurations so you don't have to create your own during development.

As mentioned in the previous section development test services run three MongoDB
connector instances. There is a separate configuration directory for each
instance. Those are in,

- `fixtures/hasura/sample_mflix/connector/`
- `fixtures/hasura/chinook/connector/`
- `fixtures/hasura/test_cases/connector/`

Connector instances are automatically restarted with updated configuration when
you run `just up`.

If you make changes to MongoDB databases you may want to run connector
introspection to automatically update configurations. See the specific
instructions in the [fixtures readme](../fixtures/hasura/README.md).

### DDN Metadata

The Hasura GraphQL Engine must be configured with DDN metadata which is
configured in `.hml` files. Once again this repo provides configuration in
`fixtures/hasura/`.

If you have made changes to MongoDB fixture data or to connector configurations
you may want to update metadata using the DDN CLI by querying connectors.
Connectors must be restarted with updated configurations before you do this. For
specific instructions see the [fixtures readme](../fixtures/hasura/README.md).

The Engine will automatically restart with updated configuration after any
changes to `.hml` files when you run `just up`.

## Docker Compose Configuration

The [`justfile`](../justfile) recipes delegate to arion which is a frontend for
docker-compose that adds a layer of convenience where it can easily load
connector code changes. If you are using the development shell you can run
`arion` commands directly. They mostly work just like `docker-compose` commands:

To start all services run:

    $ arion up -d

To recompile and restart the connector after code changes run:

    $ arion up -d connector

The arion configuration runs these services:

- connector: the MongoDB data connector agent defined in this repo serving the sample_mflix database (port 7130)
- two more instances of the connector - one connected to the chinook sample database, the other to a database of ad-hoc data that is queried by integration tests (ports 7131 & 7132)
- mongodb (port 27017)
- Hasura GraphQL Engine (HGE) (port 7100)
- a stubbed authentication server
- jaeger to collect logs (see UI at http://localhost:16686/)

Connect to the HGE GraphiQL UI at http://localhost:7100/

Instead of a `docker-compose.yaml` configuration is found in
`arion-compose.nix`. That file imports from modular configurations in the
`arion-compose/` directory. Here is a quick breakdown of those files:

```
arion-compose.nix                  -- entrypoint for interactive services configuration
arion-pkgs.nix                     -- defines the `pkgs` variable that is passed as an argument to other arion files
arion-compose
├── default.nix                    -- arion-compose.nix delegates to the function exported from this file
├── integration-tests.nix          -- entrypoint for integration test configuration
├── integration-test-services.nix  -- high-level service configurations used by interactive services, and by integration tests
├── fixtures
│  └── mongodb.nix                 -- provides a dictionary of MongoDB fixture data directories
└── services                       -- each file here exports a function that configures a specific service
   ├── connector.nix               -- configures the MongoDB connector with overridable settings
   ├── dev-auth-webhook.nix        -- stubbed authentication server
   ├── engine.nix                  -- Hasura GraphQL Engine
   ├── integration-tests.nix       -- integration test runner
   ├── jaeger.nix                  -- OpenTelemetry trace collector
   └── mongodb.nix                 -- MongoDB database server
```

## Project Maintenance Notes

### Updating GraphQL Engine for integration tests

It's important to keep the GraphQL Engine version updated to make sure that the
connector is working with the latest engine version. To update run,

```sh
$ nix flake lock --update-input graphql-engine-source
```

Then commit the changes to `flake.lock` to version control.

A specific engine version can be specified by editing `flake.lock` instead of
running the above command like this:

```diff
     graphql-engine-source = {
-      url = "github:hasura/graphql-engine";
+      url = "github:hasura/graphql-engine/<git-hash-branch-or-tag>";
       flake = false;
     };
```

### Updating Rust version

Updating the Rust version used in the Nix build system requires two steps (in
any order):

- update `rust-overlay` which provides Rust toolchains
- edit `rust-toolchain.toml` to specify the desired toolchain version

To update `rust-overlay` run,

```sh
$ nix flake lock --update-input rust-overlay
```

If you are using direnv to automatically apply the nix dev environment note that
edits to `rust-toolchain.toml` will not automatically update your environment.
You can make a temporary edit to `flake.nix` (like adding a space somewhere)
which will trigger an update, and then you can revert the change.

### Updating other project dependencies

You can update all dependencies declared in `flake.nix` at once by running,

```sh
$ nix flake update
```

This will update `graphql-engine-source` and `rust-overlay` as described above,
and will also update `advisory-db` to get updated security notices for cargo
dependencies, `nixpkgs` to get updates to openssl.
