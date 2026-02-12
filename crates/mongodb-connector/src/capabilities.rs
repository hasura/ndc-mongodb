use ndc_sdk::models::{
    AggregateCapabilities, Capabilities, DatePartScalarExpressionCapability, ExistsCapabilities,
    GroupByCapabilities, LeafCapability, NestedArrayFilterByCapabilities, NestedFieldCapabilities,
    NestedFieldFilterByCapabilities, QueryCapabilities, RelationalAggregateCapabilities,
    RelationalAggregateExpressionCapabilities, RelationalAggregateFunctionCapabilities,
    RelationalCaseCapabilities, RelationalComparisonExpressionCapabilities,
    RelationalConditionalExpressionCapabilities, RelationalExpressionCapabilities,
    RelationalJoinCapabilities, RelationalJoinTypeCapabilities,
    RelationalOrderedAggregateFunctionCapabilities, RelationalProjectionCapabilities,
    RelationalQueryCapabilities, RelationalScalarExpressionCapabilities,
    RelationalScalarTypeCapabilities, RelationalSortCapabilities, RelationalWindowCapabilities,
    RelationalWindowExpressionCapabilities, RelationshipCapabilities,
};

pub fn mongo_capabilities() -> Capabilities {
    Capabilities {
        query: QueryCapabilities {
            aggregates: Some(AggregateCapabilities {
                filter_by: None,
                group_by: Some(GroupByCapabilities {
                    filter: None,
                    order: None,
                    paginate: None,
                }),
            }),
            variables: Some(LeafCapability {}),
            explain: Some(LeafCapability {}),
            nested_fields: NestedFieldCapabilities {
                filter_by: Some(NestedFieldFilterByCapabilities {
                    nested_arrays: Some(NestedArrayFilterByCapabilities {
                        contains: Some(LeafCapability {}),
                        is_empty: Some(LeafCapability {}),
                    }),
                }),
                order_by: Some(LeafCapability {}),
                aggregates: Some(LeafCapability {}),
                nested_collections: None, // TODO: ENG-1464
            },
            exists: ExistsCapabilities {
                named_scopes: None, // TODO: ENG-1487
                unrelated: Some(LeafCapability {}),
                nested_collections: Some(LeafCapability {}),
                nested_scalar_collections: None, // TODO: ENG-1488
            },
        },
        mutation: ndc_sdk::models::MutationCapabilities {
            transactional: None,
            explain: None,
        },
        relationships: Some(RelationshipCapabilities {
            relation_comparisons: Some(LeafCapability {}),
            order_by_aggregate: None,
            nested: None, // TODO: ENG-1490
        }),
        relational_mutation: None,
        relational_query: Some(relational_query_capabilities()),
    }
}

/// Phase 2 relational query capabilities.
///
/// Supports: From, Filter, Sort, Paginate, Project relations with scalar functions.
fn relational_query_capabilities() -> RelationalQueryCapabilities {
    // Phase 2 expression capabilities - includes scalar functions
    let phase2_expression = RelationalExpressionCapabilities {
        conditional: RelationalConditionalExpressionCapabilities {
            case: Some(RelationalCaseCapabilities {
                scrutinee: Some(LeafCapability {}), // Phase 2
            }),
            nullif: Some(LeafCapability {}), // Phase 2
        },
        comparison: RelationalComparisonExpressionCapabilities {
            between: Some(LeafCapability {}),          // Phase 2
            contains: Some(LeafCapability {}),         // Phase 2
            greater_than_eq: Some(LeafCapability {}),  // Phase 1
            greater_than: Some(LeafCapability {}),     // Phase 1
            ilike: Some(LeafCapability {}),            // Phase 2
            in_list: Some(LeafCapability {}),          // Phase 1
            is_distinct_from: Some(LeafCapability {}), // Phase 2 (using $or/$and/$eq null logic)
            is_false: Some(LeafCapability {}),         // Phase 1
            is_nan: Some(LeafCapability {}),           // Phase 2
            is_null: Some(LeafCapability {}),          // Phase 1
            is_true: Some(LeafCapability {}),          // Phase 1
            is_zero: Some(LeafCapability {}),          // Phase 2
            less_than_eq: Some(LeafCapability {}),     // Phase 1
            less_than: Some(LeafCapability {}),        // Phase 1
            like: Some(LeafCapability {}),             // Phase 2
        },
        scalar: RelationalScalarExpressionCapabilities {
            abs: Some(LeafCapability {}),               // Phase 2
            and: Some(LeafCapability {}),               // Phase 1
            array_element: Some(LeafCapability {}),     // Phase 2
            binary_concat: None,                        // Not implemented
            btrim: Some(LeafCapability {}),             // Phase 2
            ceil: Some(LeafCapability {}),              // Phase 2
            character_length: Some(LeafCapability {}),  // Phase 2
            coalesce: Some(LeafCapability {}),          // Phase 2
            concat: Some(LeafCapability {}),            // Phase 2
            cos: Some(LeafCapability {}),               // Phase 2
            current_date: Some(LeafCapability {}),      // Phase 2
            current_time: Some(LeafCapability {}),      // Phase 2 (as HH:MM:SS.mmm string)
            current_timestamp: Some(LeafCapability {}), // Phase 2
            date_part: Some(DatePartScalarExpressionCapability {
                year: Some(LeafCapability {}),
                quarter: Some(LeafCapability {}),
                month: Some(LeafCapability {}),
                week: Some(LeafCapability {}),
                day_of_week: Some(LeafCapability {}),
                day_of_year: Some(LeafCapability {}),
                day: Some(LeafCapability {}),
                hour: Some(LeafCapability {}),
                minute: Some(LeafCapability {}),
                second: Some(LeafCapability {}),
                microsecond: None, // MongoDB only has millisecond precision
                millisecond: Some(LeafCapability {}),
                nanosecond: None, // MongoDB only has millisecond precision
                epoch: Some(LeafCapability {}), // Phase 2 (via $toLong / 1000)
            }), // Phase 2
            date_trunc: Some(LeafCapability {}),        // Phase 2
            divide: Some(LeafCapability {}),            // Phase 2
            exp: Some(LeafCapability {}),               // Phase 2
            floor: Some(LeafCapability {}),             // Phase 2
            get_field: Some(LeafCapability {}),         // Phase 2
            greatest: Some(LeafCapability {}),          // Phase 2
            least: Some(LeafCapability {}),             // Phase 2
            left: Some(LeafCapability {}),              // Phase 2
            ln: Some(LeafCapability {}),                // Phase 2
            log: Some(LeafCapability {}),               // Phase 2
            log10: Some(LeafCapability {}),             // Phase 2
            log2: Some(LeafCapability {}),              // Phase 2
            lpad: Some(LeafCapability {}),              // Phase 2 (using $reduce for padding)
            ltrim: Some(LeafCapability {}),             // Phase 2
            minus: Some(LeafCapability {}),             // Phase 2
            modulo: Some(LeafCapability {}),            // Phase 2
            multiply: Some(LeafCapability {}),          // Phase 2
            negate: Some(LeafCapability {}),            // Phase 2
            not: Some(LeafCapability {}),               // Phase 1
            nvl: Some(LeafCapability {}),               // Phase 2
            or: Some(LeafCapability {}),                // Phase 1
            plus: Some(LeafCapability {}),              // Phase 2
            power: Some(LeafCapability {}),             // Phase 2
            random: Some(LeafCapability {}),            // Phase 2
            replace: Some(LeafCapability {}),           // Phase 2
            reverse: Some(LeafCapability {}),           // Phase 2 (using $reverseArray + $reduce)
            right: Some(LeafCapability {}),             // Phase 2
            round: Some(LeafCapability {}),             // Phase 2
            rpad: Some(LeafCapability {}),              // Phase 2 (using $reduce for padding)
            rtrim: Some(LeafCapability {}),             // Phase 2
            sqrt: Some(LeafCapability {}),              // Phase 2
            str_pos: Some(LeafCapability {}),           // Phase 2
            substr_index: Some(LeafCapability {}),      // Phase 2 (using $split + $slice + $reduce)
            substr: Some(LeafCapability {}),            // Phase 2
            tan: Some(LeafCapability {}),               // Phase 2
            to_date: Some(LeafCapability {}),           // Phase 2
            to_lower: Some(LeafCapability {}),          // Phase 2
            to_timestamp: Some(LeafCapability {}),      // Phase 2
            to_upper: Some(LeafCapability {}),          // Phase 2
            trunc: Some(LeafCapability {}),             // Phase 2
            json_contains: Some(LeafCapability {}),     // Phase 2
            json_get: Some(LeafCapability {}),          // Phase 2
            json_get_str: Some(LeafCapability {}),      // Phase 2
            json_get_int: Some(LeafCapability {}),      // Phase 2
            json_get_float: Some(LeafCapability {}),    // Phase 2
            json_get_bool: Some(LeafCapability {}),     // Phase 2
            json_get_json: Some(LeafCapability {}),     // Phase 2
            json_as_text: Some(LeafCapability {}),      // Phase 2
            json_length: Some(LeafCapability {}),       // Phase 2
        },
        aggregate: RelationalAggregateExpressionCapabilities {
            avg: Some(LeafCapability {}),      // Phase 3
            bool_and: Some(LeafCapability {}), // Phase 3 (via $push + $allElementsTrue)
            bool_or: Some(LeafCapability {}),  // Phase 3 (via $push + $anyElementTrue)
            count: Some(RelationalAggregateFunctionCapabilities {
                distinct: Some(LeafCapability {}), // Phase 3 (via $addToSet + $size)
            }),
            first_value: Some(LeafCapability {}), // Phase 3
            last_value: Some(LeafCapability {}),  // Phase 3
            max: Some(LeafCapability {}),         // Phase 3
            median: Some(LeafCapability {}),      // Phase 3 (MongoDB 7.0+)
            min: Some(LeafCapability {}),         // Phase 3
            string_agg: Some(RelationalOrderedAggregateFunctionCapabilities {
                distinct: Some(LeafCapability {}), // Via $addToSet + $reduce
                order_by: Some(LeafCapability {}), // Via $sortArray (MongoDB 5.2+) + $reduce
            }),
            string_agg_with_separator: Some(RelationalOrderedAggregateFunctionCapabilities {
                distinct: Some(LeafCapability {}), // Via $addToSet + $reduce
                order_by: Some(LeafCapability {}), // Via $sortArray (MongoDB 5.2+) + $reduce
            }),
            sum: Some(LeafCapability {}),                    // Phase 3
            var: None, // Not supported - no MongoDB variance accumulator
            stddev: Some(LeafCapability {}), // Phase 3
            stddev_pop: Some(LeafCapability {}), // Phase 3
            approx_percentile_cont: Some(LeafCapability {}), // Phase 3 (MongoDB 7.0+)
            array_agg: Some(RelationalOrderedAggregateFunctionCapabilities {
                distinct: Some(LeafCapability {}), // Via $addToSet
                order_by: Some(LeafCapability {}), // Via $sortArray (MongoDB 5.2+)
            }),
            approx_distinct: None, // Not supported - no HyperLogLog implementation
        },
        window: RelationalWindowExpressionCapabilities {
            row_number: Some(LeafCapability {}), // Phase 5 - $documentNumber
            dense_rank: Some(LeafCapability {}), // Phase 5 - $denseRank
            ntile: None,                         // Not supported - no native MongoDB operator
            rank: Some(LeafCapability {}),       // Phase 5 - $rank
            cume_dist: None,                     // Not supported - no native MongoDB operator
            percent_rank: None,                  // Not supported - no native MongoDB operator
        },
        scalar_types: Some(RelationalScalarTypeCapabilities {
            // We support Interval literals and cast to Interval
            // (stored as { months, days, millis } document)
            interval: Some(LeafCapability {}),
            // MongoDB's type conversion operators infer the source type
            // automatically, so we support from_type by simply accepting it
            from_type: Some(LeafCapability {}),
        }),
    };

    // Sort capabilities - use same expression capabilities
    let sort_expression = phase2_expression.clone();

    // Phase 3 aggregate expression capabilities
    let aggregate_expression = phase2_expression.clone();

    // Phase 4 join expression capabilities (same as Phase 2)
    let join_expression = phase2_expression.clone();

    // Phase 5 window expression capabilities (same as Phase 2)
    let window_expression = phase2_expression.clone();

    RelationalQueryCapabilities {
        project: RelationalProjectionCapabilities {
            expression: phase2_expression.clone(),
        },
        filter: Some(phase2_expression.clone()),
        sort: Some(RelationalSortCapabilities {
            expression: sort_expression,
        }),
        join: Some(RelationalJoinCapabilities {
            expression: join_expression,
            join_types: RelationalJoinTypeCapabilities {
                // Supported join types
                left: Some(LeafCapability {}),
                inner: Some(LeafCapability {}),
                left_semi: Some(LeafCapability {}),
                left_anti: Some(LeafCapability {}),
                // Right joins supported via transformation to left joins with swapped inputs
                right: Some(LeafCapability {}),
                right_semi: Some(LeafCapability {}),
                right_anti: Some(LeafCapability {}),
                // Full outer join not supported - would require union of left join + anti join
                full: None,
            },
        }),
        aggregate: Some(RelationalAggregateCapabilities {
            expression: aggregate_expression,
            group_by: Some(LeafCapability {}), // Phase 3
        }),
        window: Some(RelationalWindowCapabilities {
            expression: window_expression, // Phase 5
        }),
        union: Some(LeafCapability {}), // Phase 6: Union via $unionWith
        streaming: Some(LeafCapability {}), // Implemented via MongoDB cursor
    }
}
