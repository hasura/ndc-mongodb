use configuration::schema::Type;
use enum_iterator::all;
use itertools::Itertools;
use mongodb_support::BsonScalarType;
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{alpha1, alphanumeric1, multispace0},
    combinator::{cut, opt, recognize},
    error::ParseError,
    multi::many0_count,
    sequence::{delimited, pair, preceded, terminated},
    IResult, Parser,
};

/// Nom parser for type expressions Parse a type expression according to GraphQL syntax, using
/// MongoDB scalar type names.
///
/// This implies that types are nullable by default unless they use the non-nullable suffix (!).
pub fn type_expression(input: &str) -> IResult<&str, Type> {
    nullability_suffix(alt((
        extended_json_annotation,
        scalar_annotation,
        predicate_annotation,
        object_annotation, // object_annotation must follow parsers that look for fixed sets of keywords
        array_of_annotation,
    )))(input)
}

fn extended_json_annotation(input: &str) -> IResult<&str, Type> {
    let (remaining, _) = tag("extendedJSON")(input)?;
    Ok((remaining, Type::ExtendedJSON))
}

fn scalar_annotation(input: &str) -> IResult<&str, Type> {
    // This parser takes the first type name that matches so in cases where one type name is
    // a prefix of another we must try the longer name first. Otherwise `javascriptWithScope` can
    // be mistaken for the type `javascript`. So we sort type names by length in descending order.
    let scalar_type_parsers = all::<BsonScalarType>()
        .sorted_by_key(|t| 1000 - t.bson_name().len())
        .map(|t| tag(t.bson_name()).map(move |_| Type::Nullable(Box::new(Type::Scalar(t)))));
    alt_many(scalar_type_parsers)(input)
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
        terminated(tag("predicate"), multispace0),
        delimited(tag("<"), cut(ws(object_type_name)), tag(">")),
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
    let (remaining, element_type) = delimited(tag("["), cut(ws(type_expression)), tag("]"))(input)?;
    Ok((
        remaining,
        Type::Nullable(Box::new(Type::ArrayOf(Box::new(element_type)))),
    ))
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
        let t = t.normalize_type(); // strip redundant nullable layers
        let (remaining, non_nullable_suffix) = opt(preceded(multispace0, tag("!")))(remaining)?;
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
pub fn alt_many<I, O, E, P, Ps>(mut parsers: Ps) -> impl FnMut(I) -> IResult<I, O, E>
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

/// A combinator that takes a parser `inner` and produces a parser that also consumes both leading and
/// trailing whitespace, returning the output of `inner`.
///
/// From https://github.com/rust-bakery/nom/blob/main/doc/nom_recipes.md#wrapper-combinators-that-eat-whitespace-before-and-after-a-parser
pub fn ws<'a, O, E: ParseError<&'a str>, F>(inner: F) -> impl Parser<&'a str, O, E>
where
    F: Parser<&'a str, O, E>,
{
    delimited(multispace0, inner, multispace0)
}

#[cfg(test)]
mod tests {
    use configuration::schema::Type;
    use googletest::prelude::*;
    use mongodb_support::BsonScalarType;
    use proptest::{prop_assert_eq, proptest};
    use test_helpers::arb_type;

    #[googletest::test]
    fn parses_scalar_type_expression() -> Result<()> {
        expect_that!(
            super::type_expression("double"),
            ok((
                anything(),
                eq(&Type::Nullable(Box::new(Type::Scalar(
                    BsonScalarType::Double
                ))))
            ))
        );
        Ok(())
    }

    #[googletest::test]
    fn parses_non_nullable_suffix() -> Result<()> {
        expect_that!(
            super::type_expression("double!"),
            ok((anything(), eq(&Type::Scalar(BsonScalarType::Double))))
        );
        Ok(())
    }

    #[googletest::test]
    fn ignores_whitespace_in_type_expressions() -> Result<()> {
        expect_that!(
            super::type_expression("[ double ! ] !"),
            ok((
                anything(),
                eq(&Type::ArrayOf(Box::new(Type::Scalar(
                    BsonScalarType::Double
                ))))
            ))
        );
        expect_that!(
            super::type_expression("predicate < obj >"),
            ok((
                anything(),
                eq(&Type::Nullable(Box::new(Type::Predicate {
                    object_type_name: "obj".into()
                })))
            ))
        );
        Ok(())
    }

    proptest! {
        #[test]
        fn type_expression_roundtrips_display_and_parsing(t in arb_type()) {
            let t = t.normalize_type();
            let annotation = t.to_string();
            println!("annotation: {}", annotation);
            let (_, parsed) = super::type_expression(&annotation)?;
            prop_assert_eq!(parsed, t)
        }
    }
}
