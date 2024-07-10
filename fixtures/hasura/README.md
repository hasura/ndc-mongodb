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

Run introspection to update connector configuration:

```sh
$ ddn connector introspect --connector sample_mflix/connector/sample_mflix/connector.local.yaml

$ ddn connector introspect --connector chinook/connector/chinook/connector.local.yaml
```

Update ddn based on connector configuration:

```sh
$ ddn connector-link update sample_mflix --subgraph sample_mflix/subgraph.yaml --env-file sample_mflix/.env.sample_mflix.local --add-all-resources

$ ddn connector-link update chinook --subgraph chinook/subgraph.yaml --env-file chinook/.env.chinook.local --add-all-resources
```
