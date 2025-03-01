# MongoDB Connector Hasura fixtures

This directory contains example DDN and connector configuration which is used to
run integration tests in this repo, and supports local development.

Instead of having docker compose configurations in this directory, supporting
services are run using arion configurations defined at the top level of the
repo. Before running ddn commands bring up services with:

```sh
arion up -d
```

## Cheat Sheet

We have three connector configurations. So a lot of these commands are repeated
for each connector.

Run introspection to update connector configuration. To do that through the ddn
CLI run these commands in the same directory as this README file:

```sh
$ ddn connector introspect sample_mflix

$ ddn connector introspect chinook

$ ddn connector introspect test_cases
```

Alternatively run `mongodb-cli-plugin` directly to use the CLI plugin version in
this repo. The plugin binary is provided by the Nix dev shell. Use these
commands:

```sh
$ nix run .#mongodb-cli-plugin -- --connection-uri mongodb://localhost/sample_mflix --context-path app/connector/sample_mflix/ update

$ nix run .#mongodb-cli-plugin -- --connection-uri mongodb://localhost/chinook --context-path app/connector/chinook/ update

$ nix run .#mongodb-cli-plugin -- --connection-uri mongodb://localhost/test_cases --context-path app/connector/test_cases/ update
```

Update Hasura metadata based on connector configuration
(after restarting connectors with `arion up -d` if there were changes from
introspection):

```sh
$ ddn connector-link update sample_mflix --add-all-resources

$ ddn connector-link update chinook --add-all-resources

$ ddn connector-link update test_cases --add-all-resources
```
