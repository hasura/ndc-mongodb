use configuration::schema::Type;
use enum_iterator::all;
use mongodb_support::BsonScalarType;
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{alpha1, alphanumeric1},
    combinator::{all_consuming, cut, opt, recognize},
    error::ParseError,
    multi::many0_count,
    sequence::{delimited, pair, preceded},
    IResult, Parser,
};

use super::error::{Error, Result};

/// Parse a type expression according to GraphQL syntax, using MongoDB scalar type names.
///
/// This implies that types are nullable by default unless they use the non-nullable suffix (!).
pub fn parse_type_annotation(input: &str) -> Result<Type> {
    match type_expression(input) {
        Ok((_, r)) => Ok(r),
        Err(err) => Err(Error::UnableToParseTypeAnnotation(format!("{err}"))),
    }
}

/// Nom parser for type expressions
pub fn type_expression(input: &str) -> IResult<&str, Type> {
    all_consuming(nullability_suffix(alt((
        extended_json_annotation,
        scalar_annotation,
        predicate_annotation,
        object_annotation, // object_annotation must follow parsers that look for fixed sets of keywords
        array_of_annotation,
    ))))(input)
}

fn extended_json_annotation(input: &str) -> IResult<&str, Type> {
    let (remaining, _) = tag("extendedJSON")(input)?;
    Ok((remaining, Type::ExtendedJSON))
}

fn scalar_annotation(input: &str) -> IResult<&str, Type> {
    let scalar_type_parsers = all::<BsonScalarType>()
        .map(|t| tag(t.bson_name()).map(move |_| Type::Nullable(Box::new(Type::Scalar(t)))));
    all_consuming(alt_many(scalar_type_parsers))(input)
}

fn object_annotation(input: &str) -> IResult<&str, Type> {
    let (remaining, name) = object_type_name(input)?;
    Ok((
        remaining,
        Type::Nullable(Box::new(Type::Object(name.into()))),
    ))
}

fn predicate_annotation(input: &str) -> IResult<&str, Type> {
    let (remaining, name) = preceded(
        tag("predicate"),
        delimited(tag("<"), cut(object_type_name), tag(">")),
    )(input)?;
    Ok((
        remaining,
        Type::Nullable(Box::new(Type::Predicate {
            object_type_name: name.into(),
        })),
    ))
}

fn object_type_name(input: &str) -> IResult<&str, &str> {
    let first_char = alt((alpha1, tag("_")));
    let succeeding_char = alt((alphanumeric1, tag("_")));
    recognize(pair(first_char, many0_count(succeeding_char)))(input)
}

fn array_of_annotation(input: &str) -> IResult<&str, Type> {
    delimited(tag("["), cut(type_expression), tag("]"))(input)
}

/// The other parsers produce nullable types by default. This wraps a parser that produces a type,
/// and flips the type from nullable to non-nullable if it sees the non-nullable suffix (!).
fn nullability_suffix<'a, P, E>(mut parser: P) -> impl FnMut(&'a str) -> IResult<&'a str, Type, E>
where
    P: Parser<&'a str, Type, E> + 'a,
    E: ParseError<&'a str>,
{
    move |input| {
        let (remaining, t) = parser.parse(input)?;
        let (remaining, non_nullable_suffix) = opt(tag("!"))(remaining)?;
        let t = match non_nullable_suffix {
            None => t,
            Some(_) => match t {
                Type::Nullable(t) => *t,
                t => t,
            },
        };
        Ok((remaining, t))
    }
}

/// Like [nom::branch::alt], but accepts a dynamically-constructed iterable of parsers instead of
/// a tuple.
///
/// From https://stackoverflow.com/a/76759023/103017
pub fn alt_many<I, O, E, P, Ps>(mut parsers: Ps) -> impl Parser<I, O, E>
where
    P: Parser<I, O, E>,
    I: Clone,
    for<'a> &'a mut Ps: IntoIterator<Item = P>,
    E: ParseError<I>,
{
    move |input: I| {
        for mut parser in &mut parsers {
            if let r @ Ok(_) = parser.parse(input.clone()) {
                return r;
            }
        }
        nom::combinator::fail::<I, O, E>(input)
    }
}

#[cfg(test)]
mod tests {
    use proptest::{prop_assert_eq, proptest};
    use test_helpers::arb_type;

    proptest! {
        #[test]
        fn test_parse_type_annotation(t in arb_type()) {
            let annotation = t.to_string();
            let parsed = super::parse_type_annotation(&annotation);
            prop_assert_eq!(parsed, t)
        }
    }
}
