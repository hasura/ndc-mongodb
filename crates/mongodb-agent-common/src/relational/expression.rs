//! Expression translation from relational expressions to MongoDB BSON.

use mongodb::bson::{bson, Bson, Decimal128};
use ndc_models::{CastType, RelationalExpression, RelationalLiteral};

use super::{ColumnMapping, RelationalError};

/// Maximum recursion depth for expression translation.
const MAX_EXPRESSION_DEPTH: u32 = 512;

/// Context for translating expressions.
pub struct ExpressionContext<'a> {
    pub column_mapping: &'a ColumnMapping,
    depth: u32,
}

impl<'a> ExpressionContext<'a> {
    pub fn new(column_mapping: &'a ColumnMapping) -> Self {
        Self {
            column_mapping,
            depth: 0,
        }
    }

    fn nested(&self) -> Result<Self, RelationalError> {
        if self.depth >= MAX_EXPRESSION_DEPTH {
            return Err(RelationalError::ExpressionTooDeep);
        }
        Ok(Self {
            column_mapping: self.column_mapping,
            depth: self.depth + 1,
        })
    }
}

/// Translate a relational expression to MongoDB BSON.
pub fn translate_expression(
    expr: &RelationalExpression,
    ctx: &ExpressionContext<'_>,
) -> Result<Bson, RelationalError> {
    let ctx = &ctx.nested()?;
    match expr {
        // Column reference
        RelationalExpression::Column { index } => {
            let field = ctx
                .column_mapping
                .field_for_index(*index)
                .ok_or(RelationalError::InvalidColumnIndex(*index))?;
            Ok(Bson::String(format!("${field}")))
        }

        // Literals
        RelationalExpression::Literal { literal } => translate_literal(literal),

        // Logical operators
        RelationalExpression::And { left, right } => {
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({ "$and": [left_bson, right_bson] }))
        }
        RelationalExpression::Or { left, right } => {
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({ "$or": [left_bson, right_bson] }))
        }
        RelationalExpression::Not { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$not": [expr_bson] }))
        }

        // Comparison operators
        RelationalExpression::Eq { left, right } => {
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({ "$eq": [left_bson, right_bson] }))
        }
        RelationalExpression::NotEq { left, right } => {
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({ "$ne": [left_bson, right_bson] }))
        }
        RelationalExpression::Lt { left, right } => {
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({ "$lt": [left_bson, right_bson] }))
        }
        RelationalExpression::LtEq { left, right } => {
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({ "$lte": [left_bson, right_bson] }))
        }
        RelationalExpression::Gt { left, right } => {
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({ "$gt": [left_bson, right_bson] }))
        }
        RelationalExpression::GtEq { left, right } => {
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({ "$gte": [left_bson, right_bson] }))
        }

        // IsDistinctFrom / IsNotDistinctFrom (NULL-safe equality)
        RelationalExpression::IsDistinctFrom { left, right } => {
            // a IS DISTINCT FROM b = NOT (a IS NOT DISTINCT FROM b)
            // a IS NOT DISTINCT FROM b = (a IS NULL AND b IS NULL) OR (a = b)
            // So: a IS DISTINCT FROM b = NOT ((a IS NULL AND b IS NULL) OR (a = b))
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({
                "$not": [{
                    "$or": [
                        { "$and": [{ "$eq": [left_bson.clone(), null] }, { "$eq": [right_bson.clone(), null] }] },
                        { "$eq": [left_bson, right_bson] }
                    ]
                }]
            }))
        }
        RelationalExpression::IsNotDistinctFrom { left, right } => {
            // (a IS NULL AND b IS NULL) OR (a = b)
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({
                "$or": [
                    { "$and": [{ "$eq": [left_bson.clone(), null] }, { "$eq": [right_bson.clone(), null] }] },
                    { "$eq": [left_bson, right_bson] }
                ]
            }))
        }

        // Null checks
        RelationalExpression::IsNull { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$eq": [expr_bson, null] }))
        }
        RelationalExpression::IsNotNull { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$ne": [expr_bson, null] }))
        }

        // Boolean checks
        RelationalExpression::IsTrue { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$eq": [expr_bson, true] }))
        }
        RelationalExpression::IsFalse { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$eq": [expr_bson, false] }))
        }
        RelationalExpression::IsNotTrue { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$ne": [expr_bson, true] }))
        }
        RelationalExpression::IsNotFalse { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$ne": [expr_bson, false] }))
        }

        // In / Not In
        RelationalExpression::In { expr, list } => {
            let expr_bson = translate_expression(expr, ctx)?;
            let list_bson: Result<Vec<Bson>, _> =
                list.iter().map(|e| translate_expression(e, ctx)).collect();
            Ok(bson!({ "$in": [expr_bson, list_bson?] }))
        }
        RelationalExpression::NotIn { expr, list } => {
            let expr_bson = translate_expression(expr, ctx)?;
            let list_bson: Result<Vec<Bson>, _> =
                list.iter().map(|e| translate_expression(e, ctx)).collect();
            Ok(bson!({ "$not": [{ "$in": [expr_bson, list_bson?] }] }))
        }

        // Arithmetic operators
        RelationalExpression::Plus { left, right } => {
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({ "$add": [left_bson, right_bson] }))
        }
        RelationalExpression::Minus { left, right } => {
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({ "$subtract": [left_bson, right_bson] }))
        }
        RelationalExpression::Multiply { left, right } => {
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({ "$multiply": [left_bson, right_bson] }))
        }
        RelationalExpression::Divide { left, right } => {
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({ "$divide": [left_bson, right_bson] }))
        }
        RelationalExpression::Modulo { left, right } => {
            let left_bson = translate_expression(left, ctx)?;
            let right_bson = translate_expression(right, ctx)?;
            Ok(bson!({ "$mod": [left_bson, right_bson] }))
        }
        RelationalExpression::Negate { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$multiply": [-1, expr_bson] }))
        }

        // Math scalar functions
        RelationalExpression::Abs { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$abs": expr_bson }))
        }
        RelationalExpression::Ceil { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$ceil": expr_bson }))
        }
        RelationalExpression::Floor { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$floor": expr_bson }))
        }
        RelationalExpression::Round { expr, prec } => {
            let expr_bson = translate_expression(expr, ctx)?;
            let prec_bson = match prec {
                Some(p) => translate_expression(p, ctx)?,
                None => Bson::Int32(0),
            };
            Ok(bson!({ "$round": [expr_bson, prec_bson] }))
        }
        RelationalExpression::Trunc { expr, prec } => {
            let expr_bson = translate_expression(expr, ctx)?;
            let prec_bson = match prec {
                Some(p) => translate_expression(p, ctx)?,
                None => Bson::Int32(0),
            };
            Ok(bson!({ "$trunc": [expr_bson, prec_bson] }))
        }
        RelationalExpression::Sqrt { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$sqrt": expr_bson }))
        }
        RelationalExpression::Power { base, exp } => {
            let base_bson = translate_expression(base, ctx)?;
            let exp_bson = translate_expression(exp, ctx)?;
            Ok(bson!({ "$pow": [base_bson, exp_bson] }))
        }
        RelationalExpression::Exp { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$exp": expr_bson }))
        }
        RelationalExpression::Ln { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$ln": expr_bson }))
        }
        RelationalExpression::Log { expr, base } => {
            let expr_bson = translate_expression(expr, ctx)?;
            let base_bson = match base {
                Some(b) => translate_expression(b, ctx)?,
                None => Bson::Int32(10), // Default to log base 10
            };
            Ok(bson!({ "$log": [expr_bson, base_bson] }))
        }
        RelationalExpression::Log10 { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$log10": expr_bson }))
        }
        RelationalExpression::Log2 { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            // log2(x) = ln(x) / ln(2)
            Ok(bson!({
                "$divide": [
                    { "$ln": expr_bson },
                    { "$ln": 2 }
                ]
            }))
        }
        RelationalExpression::Cos { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$cos": expr_bson }))
        }
        RelationalExpression::Tan { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$tan": expr_bson }))
        }
        RelationalExpression::Random => Ok(bson!({ "$rand": {} })),

        // Extended comparison operators
        RelationalExpression::Like { expr, pattern } => {
            let expr_bson = translate_expression(expr, ctx)?;
            let regex_bson = translate_like_pattern(pattern, ctx)?;
            Ok(safe_regex_match(expr_bson, regex_bson, false, false))
        }
        RelationalExpression::NotLike { expr, pattern } => {
            let expr_bson = translate_expression(expr, ctx)?;
            let regex_bson = translate_like_pattern(pattern, ctx)?;
            Ok(safe_regex_match(expr_bson, regex_bson, false, true))
        }
        RelationalExpression::ILike { expr, pattern } => {
            let expr_bson = translate_expression(expr, ctx)?;
            let regex_bson = translate_like_pattern(pattern, ctx)?;
            Ok(safe_regex_match(expr_bson, regex_bson, true, false))
        }
        RelationalExpression::NotILike { expr, pattern } => {
            let expr_bson = translate_expression(expr, ctx)?;
            let regex_bson = translate_like_pattern(pattern, ctx)?;
            Ok(safe_regex_match(expr_bson, regex_bson, true, true))
        }
        RelationalExpression::Between { low, expr, high } => {
            let low_bson = translate_expression(low, ctx)?;
            let expr_bson = translate_expression(expr, ctx)?;
            let high_bson = translate_expression(high, ctx)?;
            Ok(bson!({
                "$and": [
                    { "$gte": [expr_bson.clone(), low_bson] },
                    { "$lte": [expr_bson, high_bson] }
                ]
            }))
        }
        RelationalExpression::NotBetween { low, expr, high } => {
            let low_bson = translate_expression(low, ctx)?;
            let expr_bson = translate_expression(expr, ctx)?;
            let high_bson = translate_expression(high, ctx)?;
            Ok(bson!({
                "$or": [
                    { "$lt": [expr_bson.clone(), low_bson] },
                    { "$gt": [expr_bson, high_bson] }
                ]
            }))
        }
        RelationalExpression::Contains { str, search_str } => {
            let str_bson = translate_expression(str, ctx)?;
            let search_bson = translate_expression(search_str, ctx)?;
            Ok(bson!({
                "$gte": [
                    { "$indexOfCP": [str_bson, search_bson] },
                    0
                ]
            }))
        }
        RelationalExpression::IsNaN { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            // NaN is the only value that is not equal to itself
            Ok(bson!({
                "$and": [
                    { "$isNumber": expr_bson.clone() },
                    { "$ne": [expr_bson.clone(), expr_bson] }
                ]
            }))
        }
        RelationalExpression::IsZero { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$eq": [expr_bson, 0] }))
        }

        // Conditional scalar functions
        RelationalExpression::Case {
            scrutinee,
            when,
            default,
        } => {
            let mut branches = Vec::new();

            for when_clause in when {
                let condition_bson = translate_expression(&when_clause.when, ctx)?;
                let result_bson = translate_expression(&when_clause.then, ctx)?;

                let case_condition = match scrutinee {
                    Some(ref s) => {
                        let scrutinee_bson = translate_expression(s, ctx)?;
                        bson!({ "$eq": [scrutinee_bson, condition_bson] })
                    }
                    None => condition_bson,
                };

                branches.push(bson!({
                    "case": case_condition,
                    "then": result_bson
                }));
            }

            let default_bson = match default {
                Some(ref d) => translate_expression(d, ctx)?,
                None => Bson::Null,
            };

            Ok(bson!({
                "$switch": {
                    "branches": branches,
                    "default": default_bson
                }
            }))
        }
        RelationalExpression::Coalesce { exprs } => {
            // Build nested $ifNull: $ifNull(a, $ifNull(b, $ifNull(c, null)))
            let translated_args: Result<Vec<Bson>, _> = exprs
                .iter()
                .map(|arg| translate_expression(arg, ctx))
                .collect();
            let args_bson = translated_args?;

            // Build from right to left
            let mut result = Bson::Null;
            for arg in args_bson.into_iter().rev() {
                result = bson!({ "$ifNull": [arg, result] });
            }
            Ok(result)
        }
        RelationalExpression::NullIf { expr1, expr2 } => {
            let left_bson = translate_expression(expr1, ctx)?;
            let right_bson = translate_expression(expr2, ctx)?;
            Ok(bson!({
                "$cond": {
                    "if": { "$eq": [left_bson.clone(), right_bson] },
                    "then": Bson::Null,
                    "else": left_bson
                }
            }))
        }
        RelationalExpression::Nvl { expr1, expr2 } => {
            let expr_bson = translate_expression(expr1, ctx)?;
            let default_bson = translate_expression(expr2, ctx)?;
            Ok(bson!({ "$ifNull": [expr_bson, default_bson] }))
        }
        RelationalExpression::Greatest { exprs } => {
            let translated_args: Result<Vec<Bson>, _> = exprs
                .iter()
                .map(|arg| translate_expression(arg, ctx))
                .collect();
            Ok(bson!({ "$max": translated_args? }))
        }
        RelationalExpression::Least { exprs } => {
            let translated_args: Result<Vec<Bson>, _> = exprs
                .iter()
                .map(|arg| translate_expression(arg, ctx))
                .collect();
            Ok(bson!({ "$min": translated_args? }))
        }

        // String scalar functions
        RelationalExpression::Concat { exprs } => {
            let translated_args: Result<Vec<Bson>, _> = exprs
                .iter()
                .map(|arg| translate_expression(arg, ctx))
                .collect();
            Ok(bson!({ "$concat": translated_args? }))
        }
        RelationalExpression::CharacterLength { str } => {
            let str_bson = translate_expression(str, ctx)?;
            Ok(bson!({ "$strLenCP": str_bson }))
        }
        RelationalExpression::Substr {
            str,
            start_pos,
            len,
        } => {
            let str_bson = translate_expression(str, ctx)?;
            let start_bson = translate_expression(start_pos, ctx)?;
            // SQL SUBSTR is 1-based, MongoDB $substrCP is 0-based
            match len {
                Some(length) => {
                    let length_bson = translate_expression(length, ctx)?;
                    Ok(bson!({
                        "$substrCP": [
                            str_bson,
                            { "$subtract": [start_bson, 1] },
                            length_bson
                        ]
                    }))
                }
                None => {
                    // No length specified - go to end of string.
                    // Count = remaining chars from start position.
                    Ok(bson!({
                        "$substrCP": [
                            str_bson.clone(),
                            { "$subtract": [start_bson.clone(), 1] },
                            { "$max": [{ "$subtract": [{ "$strLenCP": str_bson }, { "$subtract": [start_bson, 1] }] }, 0] }
                        ]
                    }))
                }
            }
        }
        RelationalExpression::StrPos { str, substr } => {
            let str_bson = translate_expression(str, ctx)?;
            let substr_bson = translate_expression(substr, ctx)?;
            // $indexOfCP returns -1 if not found, we need to return 0 if not found, else 1-based position
            Ok(bson!({
                "$cond": {
                    "if": { "$eq": [{ "$indexOfCP": [str_bson.clone(), substr_bson.clone()] }, -1] },
                    "then": 0,
                    "else": { "$add": [{ "$indexOfCP": [str_bson, substr_bson] }, 1] }
                }
            }))
        }
        RelationalExpression::ToLower { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$toLower": expr_bson }))
        }
        RelationalExpression::ToUpper { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$toUpper": expr_bson }))
        }
        RelationalExpression::LTrim { str, trim_str } => {
            let str_bson = translate_expression(str, ctx)?;
            match trim_str {
                Some(chars) => {
                    let chars_bson = translate_expression(chars, ctx)?;
                    Ok(bson!({ "$ltrim": { "input": str_bson, "chars": chars_bson } }))
                }
                None => Ok(bson!({ "$ltrim": { "input": str_bson } })),
            }
        }
        RelationalExpression::RTrim { str, trim_str } => {
            let str_bson = translate_expression(str, ctx)?;
            match trim_str {
                Some(chars) => {
                    let chars_bson = translate_expression(chars, ctx)?;
                    Ok(bson!({ "$rtrim": { "input": str_bson, "chars": chars_bson } }))
                }
                None => Ok(bson!({ "$rtrim": { "input": str_bson } })),
            }
        }
        RelationalExpression::BTrim { str, trim_str } => {
            let str_bson = translate_expression(str, ctx)?;
            match trim_str {
                Some(chars) => {
                    let chars_bson = translate_expression(chars, ctx)?;
                    Ok(bson!({ "$trim": { "input": str_bson, "chars": chars_bson } }))
                }
                None => Ok(bson!({ "$trim": { "input": str_bson } })),
            }
        }
        RelationalExpression::Replace {
            str,
            substr,
            replacement,
        } => {
            let str_bson = translate_expression(str, ctx)?;
            let substr_bson = translate_expression(substr, ctx)?;
            let replacement_bson = translate_expression(replacement, ctx)?;
            Ok(bson!({
                "$replaceAll": {
                    "input": str_bson,
                    "find": substr_bson,
                    "replacement": replacement_bson
                }
            }))
        }
        RelationalExpression::Left { str, n } => {
            let str_bson = translate_expression(str, ctx)?;
            let n_bson = translate_expression(n, ctx)?;
            Ok(bson!({ "$substrCP": [str_bson, 0, n_bson] }))
        }
        RelationalExpression::Right { str, n } => {
            let str_bson = translate_expression(str, ctx)?;
            let n_bson = translate_expression(n, ctx)?;
            // Clamp start index to 0 to handle n > string length
            Ok(bson!({
                "$substrCP": [
                    str_bson.clone(),
                    { "$max": [{ "$subtract": [{ "$strLenCP": str_bson }, n_bson.clone()] }, 0] },
                    n_bson
                ]
            }))
        }
        RelationalExpression::Reverse { str } => {
            // Use $range + $map to split string into character array, $reverseArray to reverse,
            // then $reduce + $concat to join back into string
            let str_bson = translate_expression(str, ctx)?;
            Ok(bson!({
                "$reduce": {
                    "input": {
                        "$reverseArray": {
                            "$map": {
                                "input": { "$range": [0, { "$strLenCP": str_bson.clone() }] },
                                "as": "i",
                                "in": { "$substrCP": [str_bson.clone(), "$$i", 1] }
                            }
                        }
                    },
                    "initialValue": "",
                    "in": { "$concat": ["$$value", "$$this"] }
                }
            }))
        }
        RelationalExpression::LPad {
            str,
            n,
            padding_str,
        } => {
            let str_bson = translate_expression(str, ctx)?;
            let n_bson = translate_expression(n, ctx)?;
            let pad_bson = match padding_str {
                Some(p) => translate_expression(p, ctx)?,
                None => Bson::String(" ".to_string()),
            };
            // Calculate padding needed, build padding string, then concat
            Ok(bson!({
                "$let": {
                    "vars": {
                        "str": str_bson,
                        "targetLen": n_bson,
                        "padChar": pad_bson
                    },
                    "in": {
                        "$cond": {
                            "if": { "$gte": [{ "$strLenCP": "$$str" }, "$$targetLen"] },
                            "then": { "$substrCP": ["$$str", 0, "$$targetLen"] },
                            "else": {
                                "$concat": [
                                    {
                                        "$reduce": {
                                            "input": { "$range": [0, { "$subtract": ["$$targetLen", { "$strLenCP": "$$str" }] }] },
                                            "initialValue": "",
                                            "in": { "$concat": ["$$value", { "$substrCP": ["$$padChar", { "$mod": ["$$this", { "$strLenCP": "$$padChar" }] }, 1] }] }
                                        }
                                    },
                                    "$$str"
                                ]
                            }
                        }
                    }
                }
            }))
        }
        RelationalExpression::RPad {
            str,
            n,
            padding_str,
        } => {
            let str_bson = translate_expression(str, ctx)?;
            let n_bson = translate_expression(n, ctx)?;
            let pad_bson = match padding_str {
                Some(p) => translate_expression(p, ctx)?,
                None => Bson::String(" ".to_string()),
            };
            // Calculate padding needed, build padding string, then concat (string first, then padding)
            Ok(bson!({
                "$let": {
                    "vars": {
                        "str": str_bson,
                        "targetLen": n_bson,
                        "padChar": pad_bson
                    },
                    "in": {
                        "$cond": {
                            "if": { "$gte": [{ "$strLenCP": "$$str" }, "$$targetLen"] },
                            "then": { "$substrCP": ["$$str", 0, "$$targetLen"] },
                            "else": {
                                "$concat": [
                                    "$$str",
                                    {
                                        "$reduce": {
                                            "input": { "$range": [0, { "$subtract": ["$$targetLen", { "$strLenCP": "$$str" }] }] },
                                            "initialValue": "",
                                            "in": { "$concat": ["$$value", { "$substrCP": ["$$padChar", { "$mod": ["$$this", { "$strLenCP": "$$padChar" }] }, 1] }] }
                                        }
                                    }
                                ]
                            }
                        }
                    }
                }
            }))
        }
        RelationalExpression::SubstrIndex { str, delim, count } => {
            // SubstrIndex returns substring of str before count occurrences of delim
            // If count > 0: returns everything before the count-th occurrence from left
            // If count < 0: returns everything after the abs(count)-th occurrence from right
            let str_bson = translate_expression(str, ctx)?;
            let delim_bson = translate_expression(delim, ctx)?;
            let count_bson = translate_expression(count, ctx)?;
            Ok(bson!({
                "$let": {
                    "vars": {
                        "parts": { "$split": [str_bson, delim_bson.clone()] },
                        "cnt": count_bson
                    },
                    "in": {
                        "$cond": {
                            "if": { "$gte": ["$$cnt", 0] },
                            "then": {
                                "$reduce": {
                                    "input": { "$slice": ["$$parts", { "$min": ["$$cnt", { "$size": "$$parts" }] }] },
                                    "initialValue": null,
                                    "in": {
                                        "$cond": {
                                            "if": { "$eq": ["$$value", null] },
                                            "then": "$$this",
                                            "else": { "$concat": ["$$value", delim_bson.clone(), "$$this"] }
                                        }
                                    }
                                }
                            },
                            "else": {
                                "$reduce": {
                                    "input": { "$slice": ["$$parts", { "$max": [{ "$add": [{ "$size": "$$parts" }, "$$cnt"] }, 0] }, { "$size": "$$parts" }] },
                                    "initialValue": null,
                                    "in": {
                                        "$cond": {
                                            "if": { "$eq": ["$$value", null] },
                                            "then": "$$this",
                                            "else": { "$concat": ["$$value", delim_bson, "$$this"] }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }))
        }

        // JSON/object scalar functions
        RelationalExpression::GetField { column, field } => {
            let column_bson = translate_expression(column, ctx)?;
            Ok(bson!({
                "$getField": {
                    "field": field,
                    "input": column_bson
                }
            }))
        }
        RelationalExpression::ArrayElement { column, index } => {
            let column_bson = translate_expression(column, ctx)?;
            Ok(bson!({ "$arrayElemAt": [column_bson, Bson::Int64(*index as i64)] }))
        }
        RelationalExpression::JsonGet { json, keys } => {
            translate_json_path_access(json, keys, ctx, None)
        }
        RelationalExpression::JsonGetStr { json, keys } => {
            translate_json_path_access(json, keys, ctx, Some("$toString"))
        }
        RelationalExpression::JsonGetInt { json, keys } => {
            translate_json_path_access(json, keys, ctx, Some("$toInt"))
        }
        RelationalExpression::JsonGetFloat { json, keys } => {
            translate_json_path_access(json, keys, ctx, Some("$toDouble"))
        }
        RelationalExpression::JsonGetBool { json, keys } => {
            translate_json_path_access(json, keys, ctx, Some("$toBool"))
        }
        RelationalExpression::JsonGetJson { json, keys } => {
            translate_json_path_access(json, keys, ctx, None)
        }
        RelationalExpression::JsonAsText { json, keys } => {
            translate_json_path_access(json, keys, ctx, Some("$toString"))
        }
        RelationalExpression::JsonLength { json, keys } => {
            let nested = translate_json_path_access(json, keys, ctx, None)?;
            Ok(bson!({ "$size": nested }))
        }
        RelationalExpression::JsonContains { json, keys } => {
            // For JsonContains, the last key is the value to check for
            // and the preceding keys are the path to the array
            if keys.is_empty() {
                return Err(RelationalError::UnsupportedExpression(
                    "JsonContains requires at least one key".to_string(),
                ));
            }
            let (path_keys, value_key) = keys.split_at(keys.len() - 1);
            let array_expr = if path_keys.is_empty() {
                translate_expression(json, ctx)?
            } else {
                translate_json_path_access(json, path_keys, ctx, None)?
            };
            let value_bson = translate_expression(&value_key[0], ctx)?;
            Ok(bson!({ "$in": [value_bson, array_expr] }))
        }

        // Date/time scalar functions
        RelationalExpression::CurrentDate => {
            // Return current date truncated to day
            Ok(bson!({
                "$dateTrunc": {
                    "date": "$$NOW",
                    "unit": "day"
                }
            }))
        }
        RelationalExpression::CurrentTimestamp => Ok(Bson::String("$$NOW".to_string())),
        RelationalExpression::CurrentTime => {
            // Return current time as HH:MM:SS.mmm string (MongoDB has millisecond precision)
            Ok(bson!({
                "$dateToString": {
                    "format": "%H:%M:%S.%L",
                    "date": "$$NOW"
                }
            }))
        }
        RelationalExpression::DatePart { expr, part } => {
            let expr_bson = translate_expression(expr, ctx)?;
            translate_date_part(&expr_bson, part)
        }
        RelationalExpression::DateTrunc { expr, part } => {
            let expr_bson = translate_expression(expr, ctx)?;
            let part_bson = translate_expression(part, ctx)?;
            Ok(bson!({
                "$dateTrunc": {
                    "date": expr_bson,
                    "unit": part_bson
                }
            }))
        }
        RelationalExpression::ToDate { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            Ok(bson!({ "$toDate": expr_bson }))
        }
        RelationalExpression::ToTimestamp { expr } => {
            let expr_bson = translate_expression(expr, ctx)?;
            // MongoDB uses Date type for timestamps
            Ok(bson!({ "$toDate": expr_bson }))
        }

        // Cast expressions
        RelationalExpression::Cast {
            expr,
            as_type,
            from_type: _,
        } => translate_cast(expr, as_type, ctx),
        RelationalExpression::TryCast {
            expr,
            as_type,
            from_type: _,
        } => translate_try_cast(expr, as_type, ctx),

        // Unsupported expressions for Phase 1
        _ => Err(RelationalError::UnsupportedExpression(format!("{expr:?}"))),
    }
}

/// Translate an aggregate expression to MongoDB accumulator syntax for $group stage.
/// This handles aggregate functions like Count, Sum, Avg, etc.
pub fn translate_aggregate_expression(
    expr: &RelationalExpression,
    ctx: &ExpressionContext<'_>,
) -> Result<Bson, RelationalError> {
    match expr {
        // Core Aggregates (Phase 3.2)
        RelationalExpression::Count { expr, distinct } => {
            if *distinct {
                // Distinct count: collect unique values with $addToSet
                // Post-processing with $size is done in pipeline_builder
                let inner = translate_expression(expr, ctx)?;
                Ok(bson!({ "$addToSet": inner }))
            } else {
                // Check if this is COUNT(*) represented as COUNT(NULL)
                let is_count_star = matches!(
                    expr.as_ref(),
                    RelationalExpression::Literal {
                        literal: RelationalLiteral::Null,
                    }
                );
                if is_count_star {
                    // COUNT(*) - count all rows
                    Ok(bson!({ "$sum": 1 }))
                } else {
                    // COUNT(expr) - count non-null values of the expression
                    let inner = translate_expression(expr, ctx)?;
                    Ok(bson!({ "$sum": { "$cond": [{ "$ne": [inner, null] }, 1, 0] } }))
                }
            }
        }

        RelationalExpression::Sum { expr } => {
            let inner = translate_expression(expr, ctx)?;
            Ok(bson!({ "$sum": inner }))
        }

        RelationalExpression::Average { expr } => {
            let inner = translate_expression(expr, ctx)?;
            Ok(bson!({ "$avg": inner }))
        }

        RelationalExpression::Min { expr } => {
            let inner = translate_expression(expr, ctx)?;
            Ok(bson!({ "$min": inner }))
        }

        RelationalExpression::Max { expr } => {
            let inner = translate_expression(expr, ctx)?;
            Ok(bson!({ "$max": inner }))
        }

        RelationalExpression::FirstValue { expr } => {
            let inner = translate_expression(expr, ctx)?;
            Ok(bson!({ "$first": inner }))
        }

        RelationalExpression::LastValue { expr } => {
            let inner = translate_expression(expr, ctx)?;
            Ok(bson!({ "$last": inner }))
        }

        // Statistical Aggregates (Phase 3.3)
        RelationalExpression::Median { expr } => {
            let inner = translate_expression(expr, ctx)?;
            Ok(bson!({
                "$median": {
                    "input": inner,
                    "method": "approximate"
                }
            }))
        }

        RelationalExpression::Stddev { expr } => {
            let inner = translate_expression(expr, ctx)?;
            Ok(bson!({ "$stdDevSamp": inner }))
        }

        RelationalExpression::StddevPop { expr } => {
            let inner = translate_expression(expr, ctx)?;
            Ok(bson!({ "$stdDevPop": inner }))
        }

        RelationalExpression::ApproxPercentileCont { expr, percentile } => {
            let inner = translate_expression(expr, ctx)?;
            Ok(bson!({
                "$percentile": {
                    "input": inner,
                    "p": [percentile.0],
                    "method": "approximate"
                }
            }))
        }

        // Composite Aggregates (Phase 3.4)
        RelationalExpression::ArrayAgg {
            expr,
            distinct,
            order_by: _,
        } => {
            let inner = translate_expression(expr, ctx)?;
            if *distinct {
                Ok(bson!({ "$addToSet": inner }))
            } else {
                // Note: order_by is not supported in MongoDB accumulators
                Ok(bson!({ "$push": inner }))
            }
        }

        RelationalExpression::BoolAnd { expr } => {
            // Collect values with $push, then post-process with $allElementsTrue
            // Post-processing is done in pipeline_builder
            let inner = translate_expression(expr, ctx)?;
            Ok(bson!({ "$push": inner }))
        }

        RelationalExpression::BoolOr { expr } => {
            // Collect values with $push, then post-process with $anyElementTrue
            // Post-processing is done in pipeline_builder
            let inner = translate_expression(expr, ctx)?;
            Ok(bson!({ "$push": inner }))
        }

        RelationalExpression::StringAgg {
            expr,
            separator: _,
            distinct,
            order_by: _,
        } => {
            // Collect values, then post-process with $reduce + $concat
            // Post-processing (including $sortArray for order_by) is done in pipeline_builder
            let inner = translate_expression(expr, ctx)?;
            if *distinct {
                // Use $addToSet for distinct values
                Ok(bson!({ "$addToSet": inner }))
            } else {
                // Use $push to collect all values (order_by handled in post-processing)
                Ok(bson!({ "$push": inner }))
            }
        }

        // Not Supported
        RelationalExpression::Var { .. } => Err(RelationalError::UnsupportedExpression(
            "Var aggregate is not supported - MongoDB has no native variance accumulator"
                .to_string(),
        )),

        RelationalExpression::ApproxDistinct { .. } => Err(RelationalError::UnsupportedExpression(
            "ApproxDistinct aggregate is not supported - MongoDB has no HyperLogLog implementation"
                .to_string(),
        )),

        // Non-aggregate expressions should use translate_expression instead
        _ => Err(RelationalError::UnsupportedExpression(format!(
            "Expression is not an aggregate function: {expr:?}"
        ))),
    }
}

/// Translate a literal value to BSON.
fn translate_literal(literal: &RelationalLiteral) -> Result<Bson, RelationalError> {
    match literal {
        // Basic types
        RelationalLiteral::Null => Ok(Bson::Null),
        RelationalLiteral::Boolean { value } => Ok(Bson::Boolean(*value)),
        RelationalLiteral::String { value } => Ok(Bson::String(value.clone())),

        // Integer types
        RelationalLiteral::Int8 { value } => Ok(Bson::Int32(i32::from(*value))),
        RelationalLiteral::Int16 { value } => Ok(Bson::Int32(i32::from(*value))),
        RelationalLiteral::Int32 { value } => Ok(Bson::Int32(*value)),
        RelationalLiteral::Int64 { value } => Ok(Bson::Int64(*value)),
        RelationalLiteral::UInt8 { value } => Ok(Bson::Int32(i32::from(*value))),
        RelationalLiteral::UInt16 { value } => Ok(Bson::Int32(i32::from(*value))),
        RelationalLiteral::UInt32 { value } => Ok(Bson::Int64(i64::from(*value))),
        RelationalLiteral::UInt64 { value } => {
            let safe_value = i64::try_from(*value).map_err(|_| {
                RelationalError::UnsupportedExpression(
                    "UInt64 value too large for BSON Int64".to_string(),
                )
            })?;
            Ok(Bson::Int64(safe_value))
        }

        // Floating point types
        RelationalLiteral::Float32 { value } => Ok(Bson::Double(f64::from(value.0))),
        RelationalLiteral::Float64 { value } => Ok(Bson::Double(value.0)),

        // Decimal types
        RelationalLiteral::Decimal128 { value, scale, .. } => {
            // Convert i128 with scale to Decimal128
            // MongoDB Decimal128 is stored as a string representation
            let decimal = i128_to_decimal128(*value, *scale)?;
            Ok(Bson::Decimal128(decimal))
        }
        RelationalLiteral::Decimal256 { value, scale, .. } => {
            // Decimal256 value is a string, parse it with scale
            let decimal = string_to_decimal128(value, *scale)?;
            Ok(Bson::Decimal128(decimal))
        }

        // Date types - stored as days/milliseconds since Unix epoch
        RelationalLiteral::Date32 { value } => {
            // Date32: days since epoch -> convert to milliseconds
            let millis = i64::from(*value) * 86_400_000; // 24 * 60 * 60 * 1000
            Ok(Bson::DateTime(mongodb::bson::DateTime::from_millis(millis)))
        }
        RelationalLiteral::Date64 { value } => {
            // Date64: milliseconds since epoch
            Ok(Bson::DateTime(mongodb::bson::DateTime::from_millis(*value)))
        }

        // Time types - time of day since midnight
        // MongoDB doesn't have a native time-only type, so we store as milliseconds
        RelationalLiteral::Time32Second { value } => {
            // Seconds since midnight -> milliseconds
            Ok(Bson::Int64(i64::from(*value) * 1000))
        }
        RelationalLiteral::Time32Millisecond { value } => {
            // Milliseconds since midnight
            Ok(Bson::Int64(i64::from(*value)))
        }
        RelationalLiteral::Time64Microsecond { value } => {
            // Microseconds since midnight -> milliseconds
            Ok(Bson::Int64(*value / 1000))
        }
        RelationalLiteral::Time64Nanosecond { value } => {
            // Nanoseconds since midnight -> milliseconds
            Ok(Bson::Int64(*value / 1_000_000))
        }

        // Timestamp types - all convert to MongoDB DateTime (milliseconds since epoch)
        RelationalLiteral::TimestampSecond { value } => {
            let millis = value.checked_mul(1000).ok_or_else(|| {
                RelationalError::UnsupportedExpression("Timestamp overflow".to_string())
            })?;
            Ok(Bson::DateTime(mongodb::bson::DateTime::from_millis(millis)))
        }
        RelationalLiteral::TimestampMillisecond { value } => {
            Ok(Bson::DateTime(mongodb::bson::DateTime::from_millis(*value)))
        }
        RelationalLiteral::TimestampMicrosecond { value } => Ok(Bson::DateTime(
            mongodb::bson::DateTime::from_millis(*value / 1000),
        )),
        RelationalLiteral::TimestampNanosecond { value } => Ok(Bson::DateTime(
            mongodb::bson::DateTime::from_millis(*value / 1_000_000),
        )),

        // Duration types - stored as milliseconds (Int64)
        RelationalLiteral::DurationSecond { value } => {
            let millis = value.checked_mul(1000).ok_or_else(|| {
                RelationalError::UnsupportedExpression("Duration overflow".to_string())
            })?;
            Ok(Bson::Int64(millis))
        }
        RelationalLiteral::DurationMillisecond { value } => Ok(Bson::Int64(*value)),
        RelationalLiteral::DurationMicrosecond { value } => Ok(Bson::Int64(*value / 1000)),
        RelationalLiteral::DurationNanosecond { value } => Ok(Bson::Int64(*value / 1_000_000)),

        // Interval type - stored as a document with months, days, and milliseconds
        RelationalLiteral::Interval {
            months,
            days,
            nanoseconds,
        } => Ok(bson!({
            "months": *months,
            "days": *days,
            "millis": *nanoseconds / 1_000_000
        })),
    }
}

/// Convert an i128 value with scale to MongoDB Decimal128.
fn i128_to_decimal128(value: i128, scale: i8) -> Result<Decimal128, RelationalError> {
    // Format the decimal as a string and parse it
    let abs_value = value.unsigned_abs();
    let is_negative = value < 0;
    let scale_usize = scale.unsigned_abs() as usize;

    let decimal_str = if scale >= 0 {
        // Positive scale means divide by 10^scale
        let s = format!("{abs_value}");
        if scale_usize >= s.len() {
            // Need leading zeros: 123 with scale 5 -> 0.00123
            let zeros = scale_usize - s.len();
            format!(
                "{}0.{}{}",
                if is_negative { "-" } else { "" },
                "0".repeat(zeros),
                s
            )
        } else {
            // Insert decimal point: 12345 with scale 2 -> 123.45
            let split_pos = s.len() - scale_usize;
            format!(
                "{}{}.{}",
                if is_negative { "-" } else { "" },
                &s[..split_pos],
                &s[split_pos..]
            )
        }
    } else {
        // Negative scale means multiply by 10^|scale|
        format!(
            "{}{}{}",
            if is_negative { "-" } else { "" },
            abs_value,
            "0".repeat(scale_usize)
        )
    };

    decimal_str.parse::<Decimal128>().map_err(|e| {
        RelationalError::UnsupportedExpression(format!("Invalid Decimal128 value: {e}"))
    })
}

/// Convert a string decimal value with scale to MongoDB Decimal128.
fn string_to_decimal128(value: &str, scale: i8) -> Result<Decimal128, RelationalError> {
    // The value is already a string representation, but we need to apply scale
    // For Decimal256, the string is the raw integer value that needs scaling
    let parsed: i128 = value.parse().map_err(|e| {
        RelationalError::UnsupportedExpression(format!("Invalid Decimal256 string: {e}"))
    })?;
    i128_to_decimal128(parsed, scale)
}

/// Translate a DatePart expression to MongoDB BSON.
///
/// Handles two cases:
/// 1. When the input is a Date - use MongoDB date operators like $dayOfMonth, $month, etc.
/// 2. When the input is a Duration (milliseconds, from date subtraction) - calculate from millis
///
/// We use $cond with $isNumber to detect which case we're in at runtime, since MongoDB
/// date subtraction returns a Long (milliseconds) while regular date fields are Date objects.
fn translate_date_part(
    expr_bson: &Bson,
    part: &ndc_models::DatePartUnit,
) -> Result<Bson, RelationalError> {
    // Constants for time calculations (milliseconds)
    const MILLIS_PER_SECOND: i64 = 1000;
    const MILLIS_PER_MINUTE: i64 = 60 * MILLIS_PER_SECOND;
    const MILLIS_PER_HOUR: i64 = 60 * MILLIS_PER_MINUTE;
    const MILLIS_PER_DAY: i64 = 24 * MILLIS_PER_HOUR;
    const MILLIS_PER_WEEK: i64 = 7 * MILLIS_PER_DAY;

    match part {
        // For Day: if duration, calculate days from millis; if date, use $dayOfMonth
        ndc_models::DatePartUnit::Day => Ok(bson!({
            "$cond": {
                "if": { "$isNumber": expr_bson.clone() },
                "then": { "$trunc": { "$divide": [expr_bson.clone(), MILLIS_PER_DAY] } },
                "else": { "$dayOfMonth": expr_bson.clone() }
            }
        })),

        // For Hour: if duration, calculate hours from millis; if date, use $hour
        ndc_models::DatePartUnit::Hour => Ok(bson!({
            "$cond": {
                "if": { "$isNumber": expr_bson.clone() },
                "then": { "$trunc": { "$mod": [{ "$divide": [expr_bson.clone(), MILLIS_PER_HOUR] }, 24] } },
                "else": { "$hour": expr_bson.clone() }
            }
        })),

        // For Minute: if duration, calculate minutes from millis; if date, use $minute
        ndc_models::DatePartUnit::Minute => Ok(bson!({
            "$cond": {
                "if": { "$isNumber": expr_bson.clone() },
                "then": { "$trunc": { "$mod": [{ "$divide": [expr_bson.clone(), MILLIS_PER_MINUTE] }, 60] } },
                "else": { "$minute": expr_bson.clone() }
            }
        })),

        // For Second: if duration, calculate seconds from millis; if date, use $second
        ndc_models::DatePartUnit::Second => Ok(bson!({
            "$cond": {
                "if": { "$isNumber": expr_bson.clone() },
                "then": { "$trunc": { "$mod": [{ "$divide": [expr_bson.clone(), MILLIS_PER_SECOND] }, 60] } },
                "else": { "$second": expr_bson.clone() }
            }
        })),

        // For Millisecond: if duration, get millis mod 1000; if date, use $millisecond
        ndc_models::DatePartUnit::Millisecond => Ok(bson!({
            "$cond": {
                "if": { "$isNumber": expr_bson.clone() },
                "then": { "$mod": [expr_bson.clone(), MILLIS_PER_SECOND] },
                "else": { "$millisecond": expr_bson.clone() }
            }
        })),

        // For Week: if duration, calculate weeks from millis; if date, use $week
        ndc_models::DatePartUnit::Week => Ok(bson!({
            "$cond": {
                "if": { "$isNumber": expr_bson.clone() },
                "then": { "$trunc": { "$divide": [expr_bson.clone(), MILLIS_PER_WEEK] } },
                "else": { "$week": expr_bson.clone() }
            }
        })),

        // These only make sense for dates, not durations
        ndc_models::DatePartUnit::Year => Ok(bson!({ "$year": expr_bson.clone() })),
        ndc_models::DatePartUnit::Month => Ok(bson!({ "$month": expr_bson.clone() })),
        ndc_models::DatePartUnit::DayOfWeek => Ok(bson!({ "$dayOfWeek": expr_bson.clone() })),
        ndc_models::DatePartUnit::DayOfYear => Ok(bson!({ "$dayOfYear": expr_bson.clone() })),

        ndc_models::DatePartUnit::Quarter => {
            // MongoDB doesn't have $quarter, compute it: ceil(month / 3)
            Ok(bson!({
                "$ceil": {
                    "$divide": [{ "$month": expr_bson.clone() }, 3]
                }
            }))
        }

        ndc_models::DatePartUnit::Epoch => {
            // For dates: convert to milliseconds since epoch, then divide by 1000 for seconds
            // For durations: already in millis, just divide by 1000
            Ok(bson!({
                "$cond": {
                    "if": { "$isNumber": expr_bson.clone() },
                    "then": { "$divide": [expr_bson.clone(), MILLIS_PER_SECOND] },
                    "else": { "$divide": [{ "$toLong": expr_bson.clone() }, MILLIS_PER_SECOND] }
                }
            }))
        }

        _ => Err(RelationalError::UnsupportedExpression(format!(
            "DatePart with part: {part:?}"
        ))),
    }
}

/// Helper function to translate JSON path access with nested keys.
/// Chains $getField operations for each key in the path.
/// Optionally wraps the result in a type conversion operator.
fn translate_json_path_access(
    json: &RelationalExpression,
    keys: &[RelationalExpression],
    ctx: &ExpressionContext<'_>,
    conversion_operator: Option<&str>,
) -> Result<Bson, RelationalError> {
    // Start with the base JSON expression
    let mut result = translate_expression(json, ctx)?;

    // Chain $getField for each key in the path. For literal JSONPath strings
    // like "$.a.b", normalize into ["a", "b"] segments.
    for key in keys {
        if let Some(path_segments) = json_path_segments_from_key_expr(key) {
            for segment in path_segments {
                result = bson!({
                    "$getField": {
                        "field": segment,
                        "input": result
                    }
                });
            }
        } else {
            let key_bson = translate_expression(key, ctx)?;
            result = bson!({
                "$getField": {
                    "field": key_bson,
                    "input": result
                }
            });
        }
    }

    // Apply optional type conversion
    if let Some(operator) = conversion_operator {
        result = if operator == "$toString" {
            bson!({
                "$convert": {
                    "input": result,
                    "to": "string",
                    "onError": null,
                    "onNull": null
                }
            })
        } else {
            bson!({ operator: result })
        };
    }

    Ok(result)
}

/// Extract normalized JSON path segments from a key expression if it is a
/// literal string path. Supports both plain keys ("market") and JSONPath-like
/// strings ("$.market", "$.a.b").
fn json_path_segments_from_key_expr(key: &RelationalExpression) -> Option<Vec<String>> {
    let RelationalExpression::Literal {
        literal: RelationalLiteral::String { value },
    } = key
    else {
        return None;
    };

    if !value.starts_with("$.") {
        // Plain key, keep as-is.
        return Some(vec![value.clone()]);
    }

    let path = &value[2..];
    let segments = path
        .split('.')
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    Some(segments)
}

/// Translate a Cast expression to MongoDB BSON.
/// For strict casts, we use direct conversion operators like $toBool, $toString, etc.
/// These will throw an error if the conversion fails.
fn translate_cast(
    expr: &RelationalExpression,
    as_type: &CastType,
    ctx: &ExpressionContext<'_>,
) -> Result<Bson, RelationalError> {
    let expr_bson = translate_expression(expr, ctx)?;

    match as_type {
        CastType::Boolean => Ok(bson!({ "$toBool": expr_bson })),
        CastType::Utf8 => Ok(cast_to_string_bson(&expr_bson, false)),
        CastType::Int8 | CastType::Int16 | CastType::Int32 => Ok(bson!({ "$toInt": expr_bson })),
        CastType::Int64 => Ok(bson!({ "$toLong": expr_bson })),
        CastType::UInt8 | CastType::UInt16 | CastType::UInt32 => {
            // MongoDB doesn't have unsigned int types, use $toInt
            Ok(bson!({ "$toInt": expr_bson }))
        }
        CastType::UInt64 => {
            // MongoDB doesn't have unsigned long, use $toLong
            Ok(bson!({ "$toLong": expr_bson }))
        }
        CastType::Float32 | CastType::Float64 => Ok(bson!({ "$toDouble": expr_bson })),
        CastType::Decimal128 { .. } | CastType::Decimal256 { .. } => {
            Ok(bson!({ "$toDecimal": expr_bson }))
        }
        CastType::Date | CastType::Timestamp => Ok(bson!({ "$toDate": expr_bson })),
        CastType::Time => {
            // MongoDB doesn't have a native time type.
            // We extract the time-of-day from a Date as milliseconds since midnight.
            // Formula: (hour * 3600000) + (minute * 60000) + (second * 1000) + millisecond
            Ok(bson!({
                "$add": [
                    { "$multiply": [{ "$hour": expr_bson.clone() }, 3_600_000] },
                    { "$multiply": [{ "$minute": expr_bson.clone() }, 60_000] },
                    { "$multiply": [{ "$second": expr_bson.clone() }, 1_000] },
                    { "$millisecond": expr_bson }
                ]
            }))
        }
        CastType::Duration => {
            // MongoDB doesn't have a native duration type.
            // We store duration as milliseconds (Int64).
            // If input is numeric, convert to long; if it's a Date, convert to epoch millis.
            Ok(bson!({ "$toLong": expr_bson }))
        }
        CastType::Interval => {
            // MongoDB doesn't have a native interval type.
            // We represent intervals as documents with { months, days, millis }.
            // For casting, we assume the input is a numeric value representing milliseconds,
            // and create an interval with 0 months, 0 days, and the millis value.
            Ok(bson!({
                "months": 0,
                "days": 0,
                "millis": { "$toLong": expr_bson }
            }))
        }
    }
}

/// Translate a TryCast expression to MongoDB BSON.
/// For safe casts, we use $convert with onError: null to return null on conversion failure
/// instead of throwing an error.
fn translate_try_cast(
    expr: &RelationalExpression,
    as_type: &CastType,
    ctx: &ExpressionContext<'_>,
) -> Result<Bson, RelationalError> {
    let expr_bson = translate_expression(expr, ctx)?;

    // For types that can use $convert directly
    let simple_convert = |to_type: &str| {
        bson!({
            "$convert": {
                "input": expr_bson.clone(),
                "to": to_type,
                "onError": null,
                "onNull": null
            }
        })
    };

    match as_type {
        CastType::Boolean => Ok(simple_convert("bool")),
        CastType::Utf8 => Ok(cast_to_string_bson(&expr_bson, true)),
        CastType::Int8 | CastType::Int16 | CastType::Int32 => Ok(simple_convert("int")),
        CastType::Int64 => Ok(simple_convert("long")),
        CastType::UInt8 | CastType::UInt16 | CastType::UInt32 => Ok(simple_convert("int")),
        CastType::UInt64 => Ok(simple_convert("long")),
        CastType::Float32 | CastType::Float64 => Ok(simple_convert("double")),
        CastType::Decimal128 { .. } | CastType::Decimal256 { .. } => Ok(simple_convert("decimal")),
        CastType::Date | CastType::Timestamp => Ok(simple_convert("date")),
        CastType::Time => {
            // Extract time-of-day as milliseconds since midnight.
            // Use $cond to return null if the date conversion fails.
            let date_expr = bson!({
                "$convert": {
                    "input": expr_bson,
                    "to": "date",
                    "onError": null,
                    "onNull": null
                }
            });
            Ok(bson!({
                "$cond": {
                    "if": { "$eq": [date_expr.clone(), null] },
                    "then": null,
                    "else": {
                        "$add": [
                            { "$multiply": [{ "$hour": date_expr.clone() }, 3_600_000] },
                            { "$multiply": [{ "$minute": date_expr.clone() }, 60_000] },
                            { "$multiply": [{ "$second": date_expr.clone() }, 1_000] },
                            { "$millisecond": date_expr }
                        ]
                    }
                }
            }))
        }
        CastType::Duration => {
            // Convert to long (milliseconds), returning null on error
            Ok(simple_convert("long"))
        }
        CastType::Interval => {
            // Create interval document from milliseconds, returning null on error
            let millis_expr = bson!({
                "$convert": {
                    "input": expr_bson,
                    "to": "long",
                    "onError": null,
                    "onNull": null
                }
            });
            Ok(bson!({
                "$cond": {
                    "if": { "$eq": [millis_expr.clone(), null] },
                    "then": null,
                    "else": {
                        "months": 0,
                        "days": 0,
                        "millis": millis_expr
                    }
                }
            }))
        }
    }
}

/// Cast expression to string while handling object/array values.
///
/// MongoDB does not support converting objects/arrays to string with `$toString`/`$convert`.
/// For those values we return them unchanged and rely on relational response serialization
/// (which stringifies documents/arrays as JSON text). For scalar values we use normal casting.
fn cast_to_string_bson(expr_bson: &Bson, safe: bool) -> Bson {
    let scalar_string_cast = if safe {
        bson!({
            "$convert": {
                "input": expr_bson.clone(),
                "to": "string",
                "onError": null,
                "onNull": null
            }
        })
    } else {
        bson!({ "$toString": expr_bson.clone() })
    };

    bson!({
        "$cond": {
            "if": {
                "$or": [
                    { "$isArray": expr_bson.clone() },
                    { "$eq": [{ "$type": expr_bson.clone() }, "object"] }
                ]
            },
            "then": expr_bson.clone(),
            "else": scalar_string_cast
        }
    })
}

/// Convert a SQL LIKE pattern string to a regex pattern string.
///
/// SQL LIKE uses `%` for multi-character wildcard and `_` for single-character wildcard.
/// Backslash escapes a following `%` or `_` to match literally.
/// All regex metacharacters in the literal portions are escaped.
/// The result is anchored with `^` and `$` for exact match semantics.
fn sql_like_to_regex(pattern: &str) -> String {
    let mut regex = String::with_capacity(pattern.len() + 2);
    regex.push('^');

    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                // Escape character: next char is literal
                if let Some(next) = chars.next() {
                    // Escape the next char for regex if needed
                    escape_regex_char(next, &mut regex);
                } else {
                    // Trailing backslash, treat as literal
                    regex.push_str("\\\\");
                }
            }
            '%' => regex.push_str(".*"),
            '_' => regex.push('.'),
            other => escape_regex_char(other, &mut regex),
        }
    }

    regex.push('$');
    regex
}

/// Escape a single character for use in a regex pattern.
fn escape_regex_char(ch: char, out: &mut String) {
    match ch {
        '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
            out.push('\\');
            out.push(ch);
        }
        _ => out.push(ch),
    }
}

/// Translate a LIKE pattern expression into a regex BSON expression.
///
/// If the pattern is a literal string, the SQL-to-regex conversion is done at
/// translation time. For dynamic patterns, we build a MongoDB aggregation
/// expression using `$replaceAll` chains to perform the conversion at runtime.
fn translate_like_pattern(
    pattern: &RelationalExpression,
    ctx: &ExpressionContext<'_>,
) -> Result<Bson, RelationalError> {
    // If the pattern is a literal string, convert at translation time
    if let RelationalExpression::Literal {
        literal: RelationalLiteral::String { value },
    } = pattern
    {
        return Ok(Bson::String(sql_like_to_regex(value)));
    }

    // For dynamic patterns, build a MongoDB expression pipeline that:
    // 1. Escapes regex metacharacters
    // 2. Converts SQL wildcards to regex equivalents
    // 3. Anchors with ^ and $
    let pattern_bson = translate_expression(pattern, ctx)?;

    // Chain $replaceAll to escape regex metacharacters, then convert SQL wildcards.
    // We need to escape: \ . * + ? ( ) [ ] { } ^ $ |
    // Then convert % -> .* and _ -> .
    // Order matters: escape \ first, then other metacharacters, then convert wildcards.
    let escaped = chain_replace_all(
        pattern_bson,
        &[
            ("\\", "\\\\"),
            (".", "\\."),
            ("*", "\\*"),
            ("+", "\\+"),
            ("?", "\\?"),
            ("(", "\\("),
            (")", "\\)"),
            ("[", "\\["),
            ("]", "\\]"),
            ("{", "\\{"),
            ("}", "\\}"),
            ("^", "\\^"),
            ("$", "\\$"),
            ("|", "\\|"),
            ("%", ".*"),
            ("_", "."),
        ],
    );

    Ok(bson!({ "$concat": ["^", escaped, "$"] }))
}

/// Build a chain of `$replaceAll` operations.
fn chain_replace_all(input: Bson, replacements: &[(&str, &str)]) -> Bson {
    let mut result = input;
    for (find, replacement) in replacements {
        result = bson!({
            "$replaceAll": {
                "input": result,
                "find": *find,
                "replacement": *replacement
            }
        });
    }
    result
}

/// Build a LIKE/ILIKE expression that never throws when input is non-string.
///
/// MongoDB `$regexMatch` requires a string input. We first `$convert` with
/// `onError: null` / `onNull: null`, then only evaluate `$regexMatch` when the
/// conversion succeeded.
fn safe_regex_match(
    expr_bson: Bson,
    pattern_bson: Bson,
    case_insensitive: bool,
    negate: bool,
) -> Bson {
    let regex_input = bson!({
        "$convert": {
            "input": expr_bson,
            "to": "string",
            "onError": null,
            "onNull": null
        }
    });

    let regex_match = if case_insensitive {
        bson!({
            "$regexMatch": {
                "input": regex_input.clone(),
                "regex": pattern_bson,
                "options": "i"
            }
        })
    } else {
        bson!({
            "$regexMatch": {
                "input": regex_input.clone(),
                "regex": pattern_bson
            }
        })
    };

    let predicate = if negate {
        bson!({ "$not": [regex_match] })
    } else {
        regex_match
    };

    bson!({
        "$cond": {
            "if": { "$eq": [regex_input, null] },
            "then": null,
            "else": predicate
        }
    })
}
