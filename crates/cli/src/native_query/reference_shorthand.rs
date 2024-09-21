use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::{alpha1, alphanumeric1},
    combinator::{all_consuming, cut, map, opt, recognize},
    multi::{many0, many0_count},
    sequence::{delimited, pair, preceded},
    IResult,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reference {
    /// Reference to a variable that is substituted by the connector from GraphQL inputs before
    /// sending to MongoDB. For example, `"{{ artist_id }}`.
    NativeQueryVariable {
        name: String,
        variable_type: Option<String>,
    },

    /// Reference to a variable that is defined as part of the pipeline syntax. May be followed by
    /// a dot-separated path to a nested field. For example, `"$$CURRENT.foo.bar"`
    PipelineVariable {
        name: String,
        nested_path: Vec<String>,
    },

    /// Reference to a field of the input document. May be followed by a dot-separated path to
    /// a nested field. For example, `"$tomatoes.viewer.rating"`
    InputDocumentField {
        name: String,
        nested_path: Vec<String>,
    },

    /// The expression evaluates to a string - that's all we need to know
    String,
}

/// Reference shorthand is a string in an aggregation expression that may evaluate to the value of
/// a field of the input document if the string begins with $, or to a variable if it begins with
/// $$, or may be a plain string.
pub fn reference_shorthand(input: &str) -> IResult<&str, Reference> {
    all_consuming(alt((
        native_query_variable,
        pipeline_variable,
        input_document_field,
        plain_string,
    )))(input)
}

// A native query variable placeholder might be embedded in a larger string. But in that case the
// expression evaluates to a string so we ignore it.
fn native_query_variable(input: &str) -> IResult<&str, Reference> {
    let placeholder_content = |input| {
        map(take_while1(|c| c != '}' && c != '|'), |content: &str| {
            content.trim()
        })(input)
    };
    let type_annotation = preceded(tag("|"), placeholder_content);

    let (remaining, (name, variable_type)) = delimited(
        tag("{{"),
        cut(pair(placeholder_content, opt(type_annotation))),
        tag("}}"),
    )(input)?;
    // Since the native_query_variable parser runs inside an `alt`, the use of `cut` commits to
    // this branch of the `alt` after successfully parsing the opening "{{" characters.

    let variable = Reference::NativeQueryVariable {
        name: name.to_string(),
        variable_type: variable_type.map(ToString::to_string),
    };
    Ok((remaining, variable))
}

fn pipeline_variable(input: &str) -> IResult<&str, Reference> {
    let variable_parser = preceded(tag("$$"), cut(variable_name));
    let (remaining, (name, path)) = pair(variable_parser, nested_path)(input)?;
    let variable = Reference::PipelineVariable {
        name: name.to_string(),
        nested_path: path,
    };
    Ok((remaining, variable))
}

fn input_document_field(input: &str) -> IResult<&str, Reference> {
    let field_parser = preceded(tag("$"), cut(variable_name));
    let (remaining, (name, path)) = pair(field_parser, nested_path)(input)?;
    let field = Reference::InputDocumentField {
        name: name.to_string(),
        nested_path: path,
    };
    Ok((remaining, field))
}

fn variable_name(input: &str) -> IResult<&str, &str> {
    let first_char = alt((alpha1, tag("_")));
    let succeeding_char = alt((alphanumeric1, tag("_"), non_ascii1));
    recognize(pair(first_char, many0_count(succeeding_char)))(input)
}

fn nested_path(input: &str) -> IResult<&str, Vec<String>> {
    let component_parser = preceded(tag("."), take_while1(|c| c != '.'));
    let (remaining, components) = many0(component_parser)(input)?;
    Ok((
        remaining,
        components.into_iter().map(ToString::to_string).collect(),
    ))
}

fn non_ascii1(input: &str) -> IResult<&str, &str> {
    take_while1(is_non_ascii)(input)
}

fn is_non_ascii(char: char) -> bool {
    char as u8 > 127
}

fn plain_string(_input: &str) -> IResult<&str, Reference> {
    Ok(("", Reference::String))
}
