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

We have two subgraphs, and two connector configurations. So a lot of these
commands are repeated for each subgraph + connector combination.

Run introspection to update connector configuration. To do that through the ddn
CLI run these commands in the same directory as this README file:

```sh
$ ddn connector introspect --connector sample_mflix/connector/sample_mflix/connector.yaml

$ ddn connector introspect --connector chinook/connector/chinook/connector.yaml

$ ddn connector introspect --connector test_cases/connector/test_cases/connector.yaml
```

Alternatively run `mongodb-cli-plugin` directly to use the CLI plugin version in
this repo. The plugin binary is provided by the Nix dev shell. Use these
commands:

```sh
$ mongodb-cli-plugin --connection-uri mongodb://localhost/sample_mflix --context-path sample_mflix/connector/ update

$ mongodb-cli-plugin --connection-uri mongodb://localhost/chinook --context-path chinook/connector/ update

$ mongodb-cli-plugin --connection-uri mongodb://localhost/test_cases --context-path test_cases/connector/ update
```

Update Hasura metadata based on connector configuration
(after restarting connectors with `arion up -d` if there were changes from
introspection):

```sh
$ ddn connector-link update sample_mflix --subgraph sample_mflix/subgraph.yaml --env-file sample_mflix/.env.sample_mflix --add-all-resources

$ ddn connector-link update chinook --subgraph chinook/subgraph.yaml --env-file chinook/.env.chinook --add-all-resources

$ ddn connector-link update test_cases --subgraph test_cases/subgraph.yaml --env-file test_cases/.env.test_cases --add-all-resources
```
