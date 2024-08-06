# Developing

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
