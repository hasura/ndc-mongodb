# MongoDB Data Connector Docker Images

The DDN CLI can automatically create a Docker configuration for you. But if you
want to access connector Docker images directly they are available from as
`ghcr.io/hasura/ndc-mongodb`. For example,

```sh
$ docker run ghcr.io/hasura/ndc-mongodb:v1.1.0
```

The Docker images are multi-arch, supporting amd64 and arm64 Linux.

A listing of available image versions can be seen [here](https://github.com/hasura/ndc-mongodb/pkgs/container/ndc-mongodb).
