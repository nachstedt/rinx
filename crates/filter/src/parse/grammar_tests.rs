//! What the grammar accepts, and the tree each spelling produces.

use super::*;

fn parsed(input: &str) -> Expr {
    parse_filter(input).expect("expected this filter to parse")
}

fn field(name: &str) -> Operand {
    Operand::Field(FieldName::new(name).unwrap())
}

fn text(value: &str) -> Operand {
    Operand::Literal(Literal::Text(value.to_string()))
}

#[test]
fn test_an_equality_comparison_parses() {
    // Given
    let input = r#"status == "open""#;

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::Compare {
            left: field("status"),
            op: CompareOp::Eq,
            right: text("open"),
        }
    );
}

#[test]
fn test_an_inequality_comparison_parses() {
    // Given
    let input = r#"status != "closed""#;

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::Compare {
            left: field("status"),
            op: CompareOp::Ne,
            right: text("closed"),
        }
    );
}

#[test]
fn test_two_fields_may_be_compared() {
    // Given
    let input = "owner == reviewer";

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::Compare {
            left: field("owner"),
            op: CompareOp::Eq,
            right: field("reviewer"),
        }
    );
}

#[test]
fn test_an_in_test_parses_with_the_needle_first() {
    // Given — Python's order, which is what the corpus writes
    let input = r#""safety" in docname"#;

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::Contains {
            needle: text("safety"),
            haystack: field("docname"),
            negated: false,
        }
    );
}

#[test]
fn test_not_in_parses_as_a_negated_containment() {
    // Given
    let input = r#""draft" not in tags"#;

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::Contains {
            needle: text("draft"),
            haystack: field("tags"),
            negated: true,
        }
    );
}

#[test]
fn test_is_none_parses() {
    // Given
    let input = "owner is None";

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::IsNone {
            field: FieldName::new("owner").unwrap(),
            negated: false,
        }
    );
}

#[test]
fn test_is_not_none_parses_as_the_negation() {
    // Given — the single most common clause in the corpus
    let input = "docname is not None";

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::IsNone {
            field: FieldName::new("docname").unwrap(),
            negated: true,
        }
    );
}

#[test]
fn test_a_bare_field_parses_as_a_truthiness_test() {
    // Given
    let input = "tags";

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(expr, Expr::Truthy(field("tags")));
}

#[test]
fn test_boolean_and_integer_literals_parse() {
    // Given
    let inputs = ["automated == True", "automated == False", "count == 3"];

    // When
    let rights: Vec<Operand> = inputs
        .iter()
        .map(|input| match parsed(input) {
            Expr::Compare { right, .. } => right,
            other => panic!("expected a comparison, got {other:?}"),
        })
        .collect();

    // Then
    assert_eq!(
        rights,
        [
            Operand::Literal(Literal::Bool(true)),
            Operand::Literal(Literal::Bool(false)),
            Operand::Literal(Literal::Int(3)),
        ]
    );
}

#[test]
fn test_and_binds_tighter_than_or() {
    // Given — `a or b and c` must group as `a or (b and c)`, as in Python
    let input = "a or b and c";

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::Or(
            Box::new(Expr::Truthy(field("a"))),
            Box::new(Expr::And(
                Box::new(Expr::Truthy(field("b"))),
                Box::new(Expr::Truthy(field("c"))),
            )),
        )
    );
}

#[test]
fn test_and_is_left_associative() {
    // Given
    let input = "a and b and c";

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::And(
            Box::new(Expr::And(
                Box::new(Expr::Truthy(field("a"))),
                Box::new(Expr::Truthy(field("b"))),
            )),
            Box::new(Expr::Truthy(field("c"))),
        )
    );
}

#[test]
fn test_parentheses_override_precedence() {
    // Given
    let input = "(a or b) and c";

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::And(
            Box::new(Expr::Or(
                Box::new(Expr::Truthy(field("a"))),
                Box::new(Expr::Truthy(field("b"))),
            )),
            Box::new(Expr::Truthy(field("c"))),
        )
    );
}

#[test]
fn test_not_binds_tighter_than_and() {
    // Given
    let input = "not a and b";

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::And(
            Box::new(Expr::Not(Box::new(Expr::Truthy(field("a"))))),
            Box::new(Expr::Truthy(field("b"))),
        )
    );
}

#[test]
fn test_not_may_be_repeated() {
    // Given
    let input = "not not a";

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::Not(Box::new(Expr::Not(Box::new(Expr::Truthy(field("a"))))))
    );
}

#[test]
fn test_not_applies_to_a_parenthesised_group() {
    // Given
    let input = r#"not (status == "open")"#;

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::Not(Box::new(Expr::Compare {
            left: field("status"),
            op: CompareOp::Eq,
            right: text("open"),
        }))
    );
}

#[test]
fn test_whitespace_between_tokens_is_irrelevant() {
    // Given
    let spaced = r#"  status   ==   "open"  "#;
    let tight = r#"status=="open""#;

    // When
    let (from_spaced, from_tight) = (parsed(spaced), parsed(tight));

    // Then
    assert_eq!(from_spaced, from_tight);
}

#[test]
fn test_startswith_parses_into_a_prefix_test() {
    // Given — the spelling 8 slices of the benchmark corpus' pies use
    let input = r#"id.startswith("FSR_STEER")"#;

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::TextAffix {
            field: FieldName::new("id").unwrap(),
            affix: Affix::Prefix,
            text: "FSR_STEER".to_string(),
        }
    );
}

#[test]
fn test_endswith_parses_into_a_suffix_test() {
    // Given
    let input = "docname.endswith('.rst')";

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::TextAffix {
            field: FieldName::new("docname").unwrap(),
            affix: Affix::Suffix,
            text: ".rst".to_string(),
        }
    );
}

#[test]
fn test_an_affix_test_combines_with_the_rest_of_the_grammar() {
    // Given — the exact shape of `safety_example/analysis.rst:67`
    let input = r#"type == "fsr" and id.startswith("FSR_STEER")"#;

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::And(
            Box::new(Expr::Compare {
                left: field("type"),
                op: CompareOp::Eq,
                right: text("fsr"),
            }),
            Box::new(Expr::TextAffix {
                field: FieldName::new("id").unwrap(),
                affix: Affix::Prefix,
                text: "FSR_STEER".to_string(),
            }),
        )
    );
}

#[test]
fn test_a_negated_affix_test_needs_no_variant_of_its_own() {
    // Given — unlike `not in`, `not` here is an ordinary prefix operator
    let input = r#"not id.startswith("X")"#;

    // When
    let expr = parsed(input);

    // Then
    assert_eq!(
        expr,
        Expr::Not(Box::new(Expr::TextAffix {
            field: FieldName::new("id").unwrap(),
            affix: Affix::Prefix,
            text: "X".to_string(),
        }))
    );
}

#[test]
fn test_an_affix_test_reports_the_field_it_reads() {
    // Given — the parser checks every name against the entity schema, so a
    // field reachable only through a method must not escape that check
    let input = r#"id.startswith("SG_")"#;

    // When
    let expr = parsed(input);
    let names = expr.field_names();

    // Then
    assert_eq!(
        names.iter().map(ToString::to_string).collect::<Vec<_>>(),
        ["id"]
    );
}

#[test]
fn test_an_affix_test_survives_a_serialization_round_trip() {
    // Given — the expression is stored in the `.ast` and read back to render
    let expr = parsed(r#"id.endswith("_01")"#);

    // When
    let json = serde_json::to_string(&expr).unwrap();
    let decoded: Expr = serde_json::from_str(&json).unwrap();

    // Then
    assert_eq!(decoded, expr);
}
