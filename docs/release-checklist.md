# Release Checklist

## 1. Version bump PR

Create a PR in the MongoDB connector repository with these changes:

- update the `version` property in `Cargo.toml` (in the workspace root only). For example, `version = "1.5.0"`
- update `CHANGELOG.md`, add a heading under `## [Unreleased]` with the new version number and date. For example, `## [1.5.0] - 2024-12-05`
  - If any of the "Added", "Fixed", "Changed" sections are empty then delete the heading.
- update `Cargo.lock` by running `cargo check`

## 2. Tag

After the above PR is merged to `main` tag that commit. For example,

```sh
$ git tag v1.5.0
$ git push --tags
```

## 3. Publish release on Github

Pushing the tag should trigger a Github action that automatically creates
a draft release in the Github project with a changelog and binaries. (Released
docker images are pushed directly to the ghcr.io registry)

Edit the draft release, and click "Publish release"

## 4. CLI Plugins Index PR

Create a PR on https://github.com/hasura/cli-plugins-index with a title like
"Release MongoDB version 1.5.0"

This PR requires URLs and hashes for the CLI plugin for each supported platform.
Hashes are listed in the `sha256sum` asset on the Github release.

Create a new file called `plugins/ndc-mongodb/<version>/manifest.yaml`. The
plugin version number is the same as the connector version. For example,
`plugins/ndc-mongodb/v1.5.0/manifest.yaml`. Include URLs to binaries from the
Github release with matching hashes. 

Here is an example of what the new file should look like,

```yaml
name: ndc-mongodb
version: "v1.5.0"
shortDescription: "CLI plugin for Hasura ndc-mongodb"
homepage: https://hasura.io/connectors/mongodb
platforms:
  - selector: darwin-arm64
    uri: "https://github.com/hasura/ndc-mongodb/releases/download/v1.5.0/mongodb-cli-plugin-aarch64-apple-darwin"
    sha256: "449c75337cd5030074a2adc4fd4e85a677454867dd462827d894a907e6fe2031"
    bin: "hasura-ndc-mongodb"
    files:
      - from: "./mongodb-cli-plugin-aarch64-apple-darwin"
        to: "hasura-ndc-mongodb"
  - selector: linux-arm64
    uri: "https://github.com/hasura/ndc-mongodb/releases/download/v1.5.0/mongodb-cli-plugin-aarch64-unknown-linux-musl"
    sha256: "719f8c26237f7af7e7827d8f58a7142b79aa00a96d7be5d9e178898a20cbcb7c"
    bin: "hasura-ndc-mongodb"
    files:
      - from: "./mongodb-cli-plugin-aarch64-unknown-linux-musl"
        to: "hasura-ndc-mongodb"
  - selector: darwin-amd64
    uri: "https://github.com/hasura/ndc-mongodb/releases/download/v1.5.0/mongodb-cli-plugin-x86_64-apple-darwin"
    sha256: "4cea92e4dee32c604baa7f9829152b755edcdb8160f39cf699f3cb5a62d3dc50"
    bin: "hasura-ndc-mongodb"
    files:
      - from: "./mongodb-cli-plugin-x86_64-apple-darwin"
        to: "hasura-ndc-mongodb"
  - selector: windows-amd64
    uri: "https://github.com/hasura/ndc-mongodb/releases/download/v1.5.0/mongodb-cli-plugin-x86_64-pc-windows-msvc.exe"
    sha256: "a7d1117cdd6e792673946e342292e525d50a18cc833c3150190afeedd06e9538"
    bin: "hasura-ndc-mongodb.exe"
    files:
      - from: "./mongodb-cli-plugin-x86_64-pc-windows-msvc.exe"
        to: "hasura-ndc-mongodb.exe"
  - selector: linux-amd64
    uri: "https://github.com/hasura/ndc-mongodb/releases/download/v1.5.0/mongodb-cli-plugin-x86_64-unknown-linux-musl"
    sha256: "c1019d5c3dc4c4f1e39f683b590dbee3ec34929e99c97b303c6d312285a316c1"
    bin: "hasura-ndc-mongodb"
    files:
      - from: "./mongodb-cli-plugin-x86_64-unknown-linux-musl"
        to: "hasura-ndc-mongodb"
```

Values that should change for each release are,

- `.version`
- `.platforms.[].uri`
- `.platforms.[].sha256`

## 5. NDC Hub PR

Create a PR on https://github.com/hasura/ndc-hub with a title like "Release
MongoDB version 1.5.0"

### Update registry metadata

Edit `registry/hasura/mongodb/metadata.json`

- change `.overview.latest_version` to the new version, for example `v1.5.0`
- prepend an entry to the list in `.source_code.version` with a value like this:

```json
{
  "tag": "<version>",
  "hash": "<hash of tagged commit>",
  "is_verified": true
},
```

For example,

```json
{
  "tag": "v1.5.0",
  "hash": "b95da1815a9b686e517aa78f677752e36e0bfda0",
  "is_verified": true
},
```

### Add connector packaging info

Create a new file with a name of the form,
`registry/hasura/mongodb/releases/<version>/connector-packaging.json`. For
example, `registry/hasura/mongodb/releases/v1.5.0/connector-packaging.json`

The content should have this format,

```json
{
  "version": "<version>",
  "uri": "https://github.com/hasura/ndc-mongodb/releases/download/<version>/connector-definition.tgz",
  "checksum": {
    "type": "sha256",
    "value": "<content hash of connectior-definition.tgz>"
  },
  "source": {
    "hash": "<hash of tagged commit>"
  },
  "test": {
    "test_config_path": "../../tests/test-config.json"
  }
}
```

The content hash for `connector-definition.tgz` is found in the `sha256sum` file
on the Github release.

The commit hash is the same as in the previous step.

For example,

```json
{
  "version": "v1.5.0",
  "uri": "https://github.com/hasura/ndc-mongodb/releases/download/v1.5.0/connector-definition.tgz",
  "checksum": {
    "type": "sha256",
    "value": "7821513fcdc1a2689a546f20a18cdc2cce9fe218dc8506adc86eb6a2a3b256a9"
  },
  "source": {
    "hash": "b95da1815a9b686e517aa78f677752e36e0bfda0"
  },
  "test": {
    "test_config_path": "../../tests/test-config.json"
  }
}
```
