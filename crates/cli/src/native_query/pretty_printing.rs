use configuration::{schema::ObjectType, serialized::NativeQuery};
use itertools::Itertools;
use pretty::{BoxAllocator, DocAllocator, DocBuilder, Pretty};

/// Prints metadata for a native query, excluding its pipeline
pub fn pretty_print_native_query_info(
    output: &mut impl std::io::Write,
    native_query: &NativeQuery,
) -> std::io::Result<()> {
    let allocator = BoxAllocator;
    native_query_info_printer::<_, ()>(native_query, &allocator)
        .1
        .render(80, output)?;
    Ok(())
}

/// Prints metadata for a native query including its pipeline
pub fn pretty_print_native_query(
    output: &mut impl std::io::Write,
    native_query: &NativeQuery,
) -> std::io::Result<()> {
    let allocator = BoxAllocator;
    native_query_printer::<_, ()>(native_query, &allocator)
        .1
        .render(80, output)?;
    Ok(())
}

fn native_query_printer<'a, D, A>(nq: &'a NativeQuery, allocator: &'a D) -> DocBuilder<'a, D, A>
where
    D: DocAllocator<'a, A>,
    D::Doc: Clone,
    A: Clone,
{
    let info = native_query_info_printer(nq, allocator);
    let pipeline = section(
        "pipeline",
        allocator.text(serde_json::to_string_pretty(&nq.pipeline).unwrap()),
        allocator,
    );
    allocator.intersperse([info, pipeline], allocator.hardline())
}

fn native_query_info_printer<'a, D, A>(
    nq: &'a NativeQuery,
    allocator: &'a D,
) -> DocBuilder<'a, D, A>
where
    D: DocAllocator<'a, A>,
    D::Doc: Clone,
    A: Clone,
{
    let input_collection = nq.input_collection.as_ref().map(|collection| {
        allocator
            .text("input collection: ")
            .append(allocator.text(collection.to_string()))
    });

    let representation = Some(
        allocator
            .text("representation: ")
            .append(allocator.text(nq.representation.to_str())),
    );

    let parameters = if !nq.arguments.is_empty() {
        let params = nq.arguments.iter().map(|(name, definition)| {
            allocator
                .text(format!("{name}: "))
                .append(allocator.text(format!("{}", definition.r#type)))
        });
        Some(section(
            "parameters",
            allocator.intersperse(params, allocator.line()),
            allocator,
        ))
    } else {
        None
    };

    let result_type = {
        let body = if let Some(object_type) = nq.object_types.get(&nq.result_document_type) {
            object_type_printer(object_type, allocator)
        } else {
            allocator.text(nq.result_document_type.to_string())
        };
        Some(section("result type", body, allocator))
    };

    let other_object_types = nq
        .object_types
        .iter()
        .filter(|(name, _)| **name != nq.result_document_type)
        .collect_vec();
    let object_types_doc = if !other_object_types.is_empty() {
        let docs = other_object_types.into_iter().map(|(name, definition)| {
            allocator
                .text(format!("{name} "))
                .append(object_type_printer(definition, allocator))
        });
        let separator = allocator.line().append(allocator.line());
        Some(section(
            "object type definitions",
            allocator.intersperse(docs, separator),
            allocator,
        ))
    } else {
        None
    };

    allocator.intersperse(
        [
            input_collection,
            representation,
            parameters,
            result_type,
            object_types_doc,
        ]
        .into_iter()
        .filter(Option::is_some),
        allocator.hardline(),
    )
}

fn object_type_printer<'a, D, A>(ot: &'a ObjectType, allocator: &'a D) -> DocBuilder<'a, D, A>
where
    D: DocAllocator<'a, A>,
    D::Doc: Clone,
    A: Clone,
{
    let fields = ot.fields.iter().map(|(name, definition)| {
        allocator
            .text(format!("{name}: "))
            .append(allocator.text(format!("{}", definition.r#type)))
    });
    let separator = allocator.text(",").append(allocator.line());
    let body = allocator.intersperse(fields, separator);
    body.indent(2).enclose(
        allocator.text("{").append(allocator.line()),
        allocator.line().append(allocator.text("}")),
    )
}

fn section<'a, D, A>(
    heading: &'a str,
    body: impl Pretty<'a, D, A>,
    allocator: &'a D,
) -> DocBuilder<'a, D, A>
where
    D: DocAllocator<'a, A>,
    D::Doc: Clone,
    A: Clone,
{
    let heading_doc = allocator.text("## ").append(heading);
    allocator
        .line()
        .append(heading_doc)
        .append(allocator.line())
        .append(allocator.line())
        .append(body)
}
