# Contributing

_First_: if you feel insecure about how to start contributing, feel free to ask us on our
[Discord channel](https://discordapp.com/invite/hasura) in the #contrib channel. You can also just go ahead with your contribution and we'll give you feedback. Don't worry - the worst that can happen is that you'll be politely asked to change something. We appreciate any contributions, and we don't want a wall of rules to stand in the way of that.

However, for those individuals who want a bit more guidance on the best way to contribute to the project, read on. This document will cover what we're looking for. By addressing the points below, the chances that we can quickly merge or address your contributions will increase.

## 1. Code of conduct

Please follow our [Code of conduct](./code-of-conduct.md) in the context of any contributions made to Hasura.

## 2. CLA

For all contributions, a CLA (Contributor License Agreement) needs to be signed
[here](https://cla-assistant.io/hasura/ndc-mongodb) before (or after) the pull request has been submitted. A bot will prompt contributors to sign the CLA via a pull request comment, if necessary.

## 3. Ways of contributing

### Reporting an Issue

- Make sure you test against the latest released cloud version. It is possible that we may have already fixed the bug you're experiencing.
- Provide steps to reproduce the issue, including Database (e.g. MongoDB) version and Hasura DDN version.
- Please include logs, if relevant.
- Create a [issue](https://github.com/hasura/ndc-mongodb/issues/new/choose).

### Working on an issue

- We use the [fork-and-branch git workflow](https://blog.scottlowe.org/2015/01/27/using-fork-branch-git-workflow/).
- Please make sure there is an issue associated with the work that you're doing.
- If you're working on an issue, please comment that you are doing so to prevent duplicate work by others also.
- See [`development.md`](./development.md) for instructions on how to build, run, and test the connector.
- If possible format code with `rustfmt`. If your editor has a code formatting feature it probably does the right thing.
- If you're up to it we welcome updates to `CHANGELOG.md`. Notes on the change in your PR should go in the  "Unreleased" section.
