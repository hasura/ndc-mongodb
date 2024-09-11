# Support & Troubleshooting

The documentation and community will help you troubleshoot most issues. If you have encountered a bug or need to get in touch with us, you can contact us using one of the following channels:
* Support & feedback: [Discord](https://discord.gg/hasura)
* Issue & bug tracking: [GitHub issues](https://github.com/hasura/ndc-mongodb/issues)
* Follow product updates: [@HasuraHQ](https://twitter.com/hasurahq)
* Talk to us on our [website chat](https://hasura.io)

We are committed to fostering an open and welcoming environment in the community. Please see the [Code of Conduct](code-of-conduct.md).
  
If you want to report a security issue, please [read this](security.md).

## Frequently Asked Questions

### Why am I getting strings instead of numbers?

MongoDB stores data in [BSON][] format which has several numeric types:

- `double`, 64-bit floating point
- `decimal`, 128-bit floating point
- `int`, 32-bit integer
- `long`, 64-bit integer

[BSON]: https://bsonspec.org/

But GraphQL uses JSON so data must be converted from BSON to JSON in GraphQL
responses. Some JSON parsers cannot precisely decode the `decimal` and `long`
types. Specifically in JavaScript running `JSON.parse(data)` will silently
convert `decimal` and `long` values to 64-bit floats which causes loss of
precision.

If you get a `long` value that is larger than `Number.MAX_SAFE_INTEGER`
(9,007,199,254,740,991) but that is less than `Number.MAX_VALUE` (1.8e308) then
you will get a number, but it might be silently changed to a different number
than the one you should have gotten.

Some databases use `long` values as IDs - if you get loss of precision with one
of these values instead of a calculation that is a little off you might end up
with access to the wrong records.

There is a similar problem when converting a 128-bit float to a 64-bit float.
You'll get a number, but not exactly the right one.

Serializing `decimal` and `long` as strings prevents bugs that might be
difficult to detect in environments like JavaScript.

### Why am I getting data in this weird format?

You might encounter a case where you expect a simple value in GraphQL responses,
like a number or a date, but you get a weird object wrapper. For example you
might expect,

```json
{ "total": 3.0 }
```

But actually get:

```json
{ "total": { "$numberDouble": "3.0" } }
```

That weird format is [Extended JSON][]. MongoDB stores data in [BSON][] format
which includes data types that don't exist in JSON. But GraphQL responses use
JSON. Extended JSON is a means of encoding data BSON data with inline type
annotations. That provides a semi-standardized way to express, for example, date
values in JSON.

[Extended JSON]: https://www.mongodb.com/docs/manual/reference/mongodb-extended-json/

In cases where the specific type of a document field is known in your data graph
the MongoDB connector serializes values for that field using "simple" JSON which
is probably what you expect. In these cases the type of each field is known
out-of-band so inline type annotations that you would get from Extended JSON are
not necessary. But in cases where the data graph does not have a specific type
for a field (which we represent using the ExtendedJSON type in the data graph)
we serialize using Extended JSON instead to provide type information which might
be important for you.

What often happens is that when the `ddn connector introspect` command samples
your database to infer types for each collection document it encounters
different types of data under the same field name in different documents. DDN
does not support union types so we can't configure a specific type for these
cases. Instead the data schema that gets written uses the ExtendedJSON type for
those fields. 

You have two options:

#### configure a precise type for the field

Edit your connector configuration to change a type in
`schema/<collection-name>.json` to change the type of a field from
`{ "type": "extendedJSON" }` to something specific like,
`{ "type": { "scalar": "double" } }`.

#### change Extended JSON serialization settings

In your connector configuration edit `configuration.json` and change the setting
`serializationOptions` from `canonical` to `relaxed`. Extended JSON has two
serialization flavors: "relaxed" mode outputs JSON-native types like numbers as
plain values without inline type annotations. You will still see type
annotations on non-JSON-native types like dates.

## How Do I ...?

### select an entire object without listing its fields

GraphQL requires that you explicitly list all of the object fields to include in
a response. If you want to fetch entire objects the MongoDB connector provides
a workaround. The connector defines an ExtendedJSON types that represents
arbitrary BSON values. In GraphQL terms ExtendedJSON is a "scalar" type so when
you select a field of that type instead of listing nested fields you get the
entire structure, whether it's an object, an array, or anything else.

Edit the schema in your data connector configuration. (There is a schema
configuration file for each collection in the `schema/` directory). Change the
object field you want to fetch from an object type like this one:

```json
{ "type": { "object": "<object-type-name>" } }
```

Change the type to `extendedJSON`:

```json
{ "type": "extendedJSON" }
```

After restarting the connector you will also need to update metadata to
propagate the type change by running the appropriate `ddn connector-link`
command.

This is an all-or-nothing change: if a field type is ExtendedJSON you cannot
select a subset of fields. You will always get the entire structure. Also note
that fields of type ExtendedJSON are serialized according to the [Extended
JSON][] spec. (See the section above, "Why am I getting data in this weird
format?")
