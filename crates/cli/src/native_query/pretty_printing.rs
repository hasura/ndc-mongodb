use std::path::Path;

use configuration::{schema::ObjectType, serialized::NativeQuery};
use itertools::Itertools;
use pretty::{
    termcolor::{Color, ColorSpec, StandardStream},
    BoxAllocator, DocAllocator, DocBuilder, Pretty,
};
use tokio::task;

/// Prints metadata for a native query, excluding its pipeline
pub async fn pretty_print_native_query_info(
    output: &mut StandardStream,
    native_query: &NativeQuery,
) -> std::io::Result<()> {
    task::block_in_place(move || {
        let allocator = BoxAllocator;
        native_query_info_printer(native_query, &allocator)
            .1
            .render_colored(80, output)?;
        Ok(())
    })
}

/// Prints metadata for a native query including its pipeline
pub async fn pretty_print_native_query(
    output: &mut StandardStream,
    native_query: &NativeQuery,
    path: &Path,
) -> std::io::Result<()> {
    task::block_in_place(move || {
        let allocator = BoxAllocator;
        native_query_printer(native_query, path, &allocator)
            .1
            .render_colored(80, output)?;
        Ok(())
    })
}

fn native_query_printer<'a, D>(
    nq: &'a NativeQuery,
    path: &'a Path,
    allocator: &'a D,
) -> DocBuilder<'a, D, ColorSpec>
where
    D: DocAllocator<'a, ColorSpec>,
    D::Doc: Clone,
{
    let source = definition_list_entry(
        "configuration source",
        allocator.text(path.to_string_lossy()),
        allocator,
    );
    let info = native_query_info_printer(nq, allocator);
    let pipeline = section(
        "pipeline",
        allocator.text(serde_json::to_string_pretty(&nq.pipeline).unwrap()),
        allocator,
    );
    allocator.intersperse([source, info, pipeline], allocator.hardline())
}

fn native_query_info_printer<'a, D>(
    nq: &'a NativeQuery,
    allocator: &'a D,
) -> DocBuilder<'a, D, ColorSpec>
where
    D: DocAllocator<'a, ColorSpec>,
    D::Doc: Clone,
{
    let input_collection = nq.input_collection.as_ref().map(|collection| {
        definition_list_entry(
            "input collection",
            allocator.text(collection.to_string()),
            allocator,
        )
    });

    let representation = Some(definition_list_entry(
        "representation",
        allocator.text(nq.representation.to_str()),
        allocator,
    ));

    let parameters = if !nq.arguments.is_empty() {
        let params = nq.arguments.iter().map(|(name, definition)| {
            allocator
                .text(name.to_string())
                .annotate(field_name())
                .append(allocator.text(": "))
                .append(
                    allocator
                        .text(definition.r#type.to_string())
                        .annotate(type_expression()),
                )
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
                .annotate(object_type_name())
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

fn object_type_printer<'a, D>(ot: &'a ObjectType, allocator: &'a D) -> DocBuilder<'a, D, ColorSpec>
where
    D: DocAllocator<'a, ColorSpec>,
    D::Doc: Clone,
{
    let fields = ot.fields.iter().map(|(name, definition)| {
        allocator
            .text(name.to_string())
            .annotate(field_name())
            .append(allocator.text(": "))
            .append(
                allocator
                    .text(definition.r#type.to_string())
                    .annotate(type_expression()),
            )
    });
    let separator = allocator.text(",").append(allocator.line());
    let body = allocator.intersperse(fields, separator);
    body.indent(2).enclose(
        allocator.text("{").append(allocator.line()),
        allocator.line().append(allocator.text("}")),
    )
}

fn definition_list_entry<'a, D>(
    label: &'a str,
    body: impl Pretty<'a, D, ColorSpec>,
    allocator: &'a D,
) -> DocBuilder<'a, D, ColorSpec>
where
    D: DocAllocator<'a, ColorSpec>,
    D::Doc: Clone,
{
    allocator
        .text(label)
        .annotate(definition_list_label())
        .append(allocator.text(": "))
        .append(body)
}

fn section<'a, D>(
    heading: &'a str,
    body: impl Pretty<'a, D, ColorSpec>,
    allocator: &'a D,
) -> DocBuilder<'a, D, ColorSpec>
where
    D: DocAllocator<'a, ColorSpec>,
    D::Doc: Clone,
{
    let heading_doc = allocator
        .text("## ")
        .append(heading)
        .annotate(section_heading());
    allocator
        .line()
        .append(heading_doc)
        .append(allocator.line())
        .append(allocator.line())
        .append(body)
}

fn section_heading() -> ColorSpec {
    let mut color = ColorSpec::new();
    color.set_fg(Some(Color::Red));
    color.set_bold(true);
    color
}

fn definition_list_label() -> ColorSpec {
    let mut color = ColorSpec::new();
    color.set_fg(Some(Color::Blue));
    color
}

fn field_name() -> ColorSpec {
    let mut color = ColorSpec::new();
    color.set_fg(Some(Color::Yellow));
    color
}

fn object_type_name() -> ColorSpec {
    // placeholder in case we want styling here in the future
    ColorSpec::new()
}

fn type_expression() -> ColorSpec {
    // placeholder in case we want styling here in the future
    ColorSpec::new()
}
