---
name: Native Query Support
about: Report problems generating native query configurations using the CLI
title: "[Native Query]"
labels: native query
assignees: ''

---

<!-- This is the template to use if you ran a command of the form, `ddn connector plugin -- native-query create query.json` and it didn't do what you wanted. -->

### Connector version

<!-- Which MongoDB connector version are you using? This is the version of the Hasura data connector, not the version of your database server. -->

### What form of error did you see?

<!-- Replace `[ ]` with `[x]` next to each applicable list item. -->

- [ ] Type inference is not currently implemented for stage / query predicate operator / aggregation operator
- [ ] Cannot infer types for this pipeline
- [ ] Type mismatch
- [ ] Could not read aggregation pipeline
- [ ] other error
- [ ] I have feedback that does not relate to a specific error

### Error or feedback details

<!-- Please paste output from the error or errors. Or if you have feedback that is not related to an error please tell us about it here. -->

### What did you want to happen?

<!-- For example if you got a "cannot infer types" or a "type mismatch" error, what types do you think would be appropriate to infer? If you are here because of a "type inefrence is not currently implemented" error are there details you think would be helpful for us to know about features that would be useful for you for us to support? -->

### Command and pipeline

<!-- Please paste the command that you ran, and your aggregation pipeline (the content of the json file that you provide to the `create` command). -->

### Schema

<!-- If your native query uses an input collection, specified by the `--collection` flag, it is helpful for us to know as much as possible about your connector's schema configuration for that collection. If it does not contain sensitive information please paste or provide a gist link to `app/connector/<connector-name>/schema/<collection-name>.json` -->

<!-- If you are not able to share your schema file please describe configured types of any collection fields referenced in your pipeline, including whether those types are nullable. -->

<!-- If your pipeline includes `$lookup` or `$graphLookup` stages that reference other collections please provide the same information for those collections. -->

### Other details

<!-- If you have any other information, feedback, or questions please let us know. -->
