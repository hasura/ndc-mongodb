
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
