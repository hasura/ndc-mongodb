# Hasura MongoDB Connector

This repo provides a service that connects [Hasura v3][] to MongoDB databases.
Supports MongoDB 6 or later.

[Hasura v3]: https://hasura.io/

## Docker Images

The MongoDB connector is available from the [Hasura connectors directory][].
There are also Docker images available at:

https://github.com/hasura/ndc-mongodb/pkgs/container/ndc-mongodb

The published Docker images are multi-arch, supporting amd64 and arm64 Linux.

[Hasura connectors directory]: https://hasura.io/connectors/mongodb

## Build Requirements

The easiest way to set up build and development dependencies for this project is
to use Nix. If you don't already have Nix we recommend the [Determinate Systems
Nix Installer][] which automatically applies settings required by this project.

[Determinate Systems Nix Installer]: https://github.com/DeterminateSystems/nix-installer/blob/main/README.md

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

If you are using the development shell (see below) the `just` command will be
provided automatically.

Run the above command again to restart after making code changes.

## Build

To build the MongoDB connector run,

```sh
$ nix build --print-build-logs && cp result/bin/mongodb-connector <dest>
```

To cross-compile statically-linked binaries for x86_64 or ARM for Linux run,

```sh
$ nix build .#mongo-connector-x86_64-linux --print-build-logs && cp result/bin/mongodb-connector <dest>
$ nix build .#mongo-connector-aarch64-linux --print-build-logs && cp result/bin/mongodb-connector <dest>
```

The Nix configuration outputs Docker images in `.tar.gz` files. You can use
`docker load -i` to install these to the local machine's docker daemon. But it
may be more helpful to use `skopeo` for this purpose so that you can apply
a chosen tag, or override the image name.

To build and install a Docker image locally (you can change
`mongodb-connector:1.2.3` to whatever image name and tag you prefer),

```sh
$ nix build .#docker --print-build-logs \
  && skopeo --insecure-policy copy docker-archive:result docker-daemon:mongo-connector:1.2.3
```

To build a Docker image with a cross-compiled ARM binary,

```sh
$ nix build .#docker-aarch64-linux --print-build-logs \
  && skopeo --insecure-policy copy docker-archive:result docker-daemon:mongo-connector:1.2.3
```

If you don't want to install `skopeo` you can run it through Nix, `nix run
nixpkgs#skopeo -- --insecure-policy copy docker-archive:result docker-daemon:mongo-connector:1.2.3`


## Developing

### The development shell

This project uses a development shell configured in `flake.nix` that automatically
loads specific version of Rust along with all other project dependencies. The
simplest way to start a development shell is with this command:

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

### Running the Connector During Development

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

The `justfile` delegates to arion which is a frontend for docker-compose that
adds a layer of convenience where it can easily load agent code changes. If you
are using the devShell you can run `arion` commands directly. They mostly work
just like `docker-compose` commands:

To start all services run:

    $ arion up -d

To recompile and restart the connector after code changes run:

    $ arion up -d connector

The arion configuration runs these services:

- connector: the MongoDB data connector agent defined in this repo (port 7130)
- mongodb
- Hasura GraphQL Engine
- a stubbed authentication server
- jaeger to collect logs (see UI at http://localhost:16686/)

Connect to the HGE GraphiQL UI at http://localhost:7100/

Instead of a `docker-compose.yaml` configuration is found in `arion-compose.nix`.

### Working with Test Data

The arion configuration in the previous section preloads MongoDB with test data.
There is corresponding OpenDDN configuration in the `fixtures/hasura/`
directory.

Preloaded databases are populated by scripts in `fixtures/mongodb/`. Any `.js`
or `.sh` scripts added to this directory will be run when the mongodb service is
run from a fresh state. Note that you will have to remove any existing docker
volume to get to a fresh state. Using arion you can remove volumes by running
`arion down --volumes`.

### Running with a different MongoDB version

Override the MongoDB version that arion runs by assigning a Docker image name to
the environment variable `MONGODB_IMAGE`. For example,

    $ arion down --volumes # delete potentially-incompatible MongoDB data
    $ MONGODB_IMAGE=mongo:6 arion up -d

Or run integration tests against a specific MongoDB version,

    $ MONGODB_IMAGE=mongo:6 just test-integration

## License

The Hasura MongoDB Connector is available under the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0) (Apache-2.0).
