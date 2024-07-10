# Hasura MongoDB Connector

## Requirements

* Rust via Rustup
* MongoDB `>= 6`
* OpenSSL development files

or get dependencies automatically with Nix

Some of the build instructions require Nix. To set that up [install Nix][], and
configure it to [enable flakes][].

[install Nix]: https://nixos.org/download.html
[enable flakes]: https://nixos.wiki/wiki/Flakes

## Build & Run

To build a statically-linked binary run,

```sh
$ nix build --print-build-logs && cp result/bin/mongodb-connector <dest>
```

To cross-compile a statically-linked ARM build for Linux run,

```sh
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

This project uses a devShell configuration in `flake.nix` that automatically
loads specific version of Rust, mongosh, and other utilities. The easiest way to
make use of the devShell is to install nix, direnv and nix-direnv. See
https://github.com/nix-community/nix-direnv

Direnv will source `.envrc`, install the appropriate Nix packages automatically
(isolated from the rest of your system packages), and configure your shell to
use the project dependencies when you cd into the project directory. All shell
modifications are reversed when you navigate to another directory.

### Running the Connector During Development

If you have set up nix and direnv then you can use arion to run the agent with
all of the services that it needs to function. Arion is a frontend for
docker-compose that adds a layer of convenience where it can easily load agent
code changes. It is automatically included with the project's devShell.

To start all services run:

    $ arion up -d

To recompile and restart the agent after code changes run:

    $ arion up -d connector

Arion delegates to docker-compose so it uses the same subcommands with the same
flags. Note that the PostgreSQL and MongoDB services use persistent volumes so
if you want to completely reset the state of those services you will need to
remove volumes using the `docker volume rm` command.

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
There is corresponding OpenDDN configuration in the `fixtures/` directory.

The preloaded data is in the form of scripts in `fixtures/mongodb/`. Any `.js`
or `.sh` scripts added to this directory will be run when the mongodb service is
run from a fresh state. Note that you will have to remove any existing docker
volume to get to a fresh state. Using arion you can remove volumes by running
`arion down`.

### Running with a different MongoDB version

Override the MongoDB version that arion runs by assigning a Docker image name to
the environment variable `MONGODB_IMAGE`. For example,

    $ arion down --volumes # delete potentially-incompatible MongoDB data
    $ MONGODB_IMAGE=mongo:4 arion up -d

Or run integration tests against a specific MongoDB version,

    $ MONGODB_IMAGE=mongo:4 just test-integration

## License

The Hasura MongoDB Connector is available under the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0) (Apache-2.0).
