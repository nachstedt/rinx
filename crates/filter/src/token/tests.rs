use super::*;

/// The kinds of `input`, dropping positions — for tests about *what* was read.
fn kinds(input: &str) -> Vec<TokenKind> {
    tokenize(input)
        .unwrap()
        .into_iter()
        .map(|token| token.kind)
        .collect()
}

/// The error `input` fails with.
fn error(input: &str) -> FilterError {
    tokenize(input).expect_err("expected the tokenizer to reject this input")
}

fn ident(name: &str) -> TokenKind {
    TokenKind::Ident(name.to_string())
}

fn string(value: &str) -> TokenKind {
    TokenKind::Str(value.to_string())
}

#[test]
fn test_an_empty_expression_yields_no_tokens() {
    // Given
    let input = "   ";

    // When
    let tokens = tokenize(input).unwrap();

    // Then
    assert!(tokens.is_empty());
}

#[test]
fn test_a_comparison_reads_as_three_tokens() {
    // Given — the shape 15 of the corpus' 17 filters open with
    let input = r#"type == "fsr""#;

    // When
    let read = kinds(input);

    // Then
    assert_eq!(read, [ident("type"), TokenKind::EqEq, string("fsr")]);
}

#[test]
fn test_keywords_are_recognised_lexically() {
    // Given
    let input = "and or not in is True False None";

    // When
    let read = kinds(input);

    // Then
    assert_eq!(
        read,
        [
            TokenKind::And,
            TokenKind::Or,
            TokenKind::Not,
            TokenKind::In,
            TokenKind::Is,
            TokenKind::True,
            TokenKind::False,
            TokenKind::NoneLit,
        ]
    );
}

#[test]
fn test_a_word_that_merely_starts_with_a_keyword_is_a_field() {
    // Given — `information` must not read as `in` followed by `formation`
    let input = "information";

    // When
    let read = kinds(input);

    // Then
    assert_eq!(read, [ident("information")]);
}

#[test]
fn test_a_field_name_may_hold_a_hyphen() {
    // Given — a schema may declare `safety-level`, and this grammar has no
    // subtraction for the hyphen to be confused with
    let input = "safety-level";

    // When
    let read = kinds(input);

    // Then
    assert_eq!(read, [ident("safety-level")]);
}

#[test]
fn test_single_quoted_strings_are_accepted() {
    // Given — Python allows both spellings, so refusing one would reject a
    // filter that is otherwise entirely supported
    let input = "'open'";

    // When
    let read = kinds(input);

    // Then
    assert_eq!(read, [string("open")]);
}

#[test]
fn test_a_string_may_hold_the_other_quote_character() {
    // Given
    let input = r#"'a "quoted" word'"#;

    // When
    let read = kinds(input);

    // Then
    assert_eq!(read, [string("a \"quoted\" word")]);
}

#[test]
fn test_backslash_escapes_are_resolved() {
    // Given
    let input = r#""a\"b\nc\\d""#;

    // When
    let read = kinds(input);

    // Then
    assert_eq!(read, [string("a\"b\nc\\d")]);
}

#[test]
fn test_an_unknown_escape_stands_for_itself() {
    // Given — Python leaves an unrecognised escape alone rather than failing
    let input = r#""a\qb""#;

    // When
    let read = kinds(input);

    // Then
    assert_eq!(read, [string("aqb")]);
}

#[test]
fn test_an_unterminated_string_is_reported_from_its_opening_quote() {
    // Given
    let input = r#"title == "boot"#;

    // When
    let failure = error(input);

    // Then
    assert_eq!(failure.kind, FilterErrorKind::UnterminatedString);
    assert_eq!(failure.offset, 9);
}

#[test]
fn test_integers_are_read_with_their_sign() {
    // Given
    let input = "count == -2";

    // When
    let read = kinds(input);

    // Then
    assert_eq!(read, [ident("count"), TokenKind::EqEq, TokenKind::Int(-2)]);
}

#[test]
fn test_a_minus_between_spaces_is_arithmetic_rather_than_a_sign() {
    // Given
    let input = "count == a - b";

    // When
    let failure = error(input);

    // Then
    assert_eq!(failure.kind, FilterErrorKind::unsupported("arithmetic"));
}

#[test]
fn test_a_fractional_number_is_refused_by_name() {
    // Given — this language has no floating-point type, and truncating to `1`
    // would silently change what the filter selects
    let input = "ratio == 1.5";

    // When
    let failure = error(input);

    // Then
    assert!(matches!(
        failure.kind,
        FilterErrorKind::Unsupported {
            construct: "non-integer numbers",
            ..
        }
    ));
}

#[test]
fn test_a_number_too_large_for_an_i64_is_reported_as_such() {
    // Given
    let input = "count == 99999999999999999999";

    // When
    let failure = error(input);

    // Then
    assert!(matches!(failure.kind, FilterErrorKind::NumberOutOfRange(_)));
}

#[test]
fn test_parentheses_are_their_own_tokens() {
    // Given
    let input = "(a)";

    // When
    let read = kinds(input);

    // Then
    assert_eq!(read, [TokenKind::LParen, ident("a"), TokenKind::RParen]);
}

#[test]
fn test_offsets_count_characters_not_bytes() {
    // Given — a column is counted in characters everywhere in this build, so a
    // multi-byte character before the fault must not shift the offset
    let input = r#"title == "ü" and x"#;

    // When
    let tokens = tokenize(input).unwrap();

    // Then
    let last = tokens.last().unwrap();
    assert_eq!(last.kind, ident("x"));
    assert_eq!(last.offset, 17);
}

#[test]
fn test_a_token_reports_its_own_length() {
    // Given
    let input = "status == 'open'";

    // When
    let tokens = tokenize(input).unwrap();

    // Then
    let lengths: Vec<usize> = tokens.iter().map(|token| token.length).collect();
    assert_eq!(lengths, [6, 2, 6]);
}

#[test]
fn test_python_boolean_operators_are_rejected_with_the_spelling_to_use() {
    // Given
    let cases = [
        ("a && b", "`&&`", "use `and`"),
        ("a || b", "`||`", "use `or`"),
    ];

    // When
    let failures: Vec<FilterError> = cases.iter().map(|(input, _, _)| error(input)).collect();

    // Then
    for (failure, (_, construct, hint)) in failures.iter().zip(cases) {
        assert_eq!(
            failure.kind,
            FilterErrorKind::unsupported_but(construct, hint)
        );
    }
}

#[test]
fn test_a_bare_bang_is_rejected_in_favour_of_not() {
    // Given
    let input = "!closed";

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::unsupported_but("`!`", "use `not`")
    );
}

#[test]
fn test_ordering_comparisons_are_rejected_by_name() {
    // Given — every spelling, since `>=` must not read as `>` plus `=`
    let inputs = ["a > 1", "a < 1", "a >= 1", "a <= 1"];

    // When
    let failures: Vec<FilterErrorKind> = inputs.iter().map(|input| error(input).kind).collect();

    // Then
    assert!(failures.iter().all(|kind| matches!(
        kind,
        FilterErrorKind::Unsupported {
            construct: "ordering comparisons",
            ..
        }
    )));
}

#[test]
fn test_a_dynamic_function_is_reported_as_one() {
    // Given — sphinx-needs' `[[copy('id')]]`
    let input = "id == [[copy('id')]]";

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::unsupported("dynamic functions")
    );
    assert_eq!(failure.offset, 6);
}

#[test]
fn test_a_list_literal_is_reported_separately_from_a_dynamic_function() {
    // Given
    let input = "status in ['open']";

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::unsupported("list literals and comprehensions")
    );
}

#[test]
fn test_a_comprehension_keyword_is_reported_by_name() {
    // Given
    let input = "n for n in needs";

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::unsupported("comprehensions and conditional expressions")
    );
    assert_eq!(failure.offset, 2);
}

#[test]
fn test_a_dot_is_read_as_a_token_rather_than_rejected_here() {
    // Given — the tokenizer cannot tell `links.id` from `id.startswith("a")`;
    // both open the same way, so the parser is what decides between them
    let input = "links.id == 'X'";

    // When
    let read = kinds(input);

    // Then
    assert_eq!(
        read,
        [
            ident("links"),
            TokenKind::Dot,
            ident("id"),
            TokenKind::EqEq,
            string("X"),
        ]
    );
}

#[test]
fn test_a_tuple_is_rejected_with_the_operator_to_use_instead() {
    // Given
    let input = "a == 'x', b == 'y'";

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::unsupported_but("tuples", "combine conditions with `and` or `or`")
    );
}

#[test]
fn test_a_single_equals_suggests_the_comparison_operator() {
    // Given
    let input = "status = 'open'";

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::unsupported_but("assignment", "use `==` to compare")
    );
}

#[test]
fn test_an_f_string_is_rejected_before_it_looks_like_a_field() {
    // Given — without this, `f"x"` would read as the field `f` beside a string
    let input = r#"title == f"x""#;

    // When
    let failure = error(input);

    // Then
    assert!(matches!(
        failure.kind,
        FilterErrorKind::Unsupported {
            construct: "string prefixes such as f-strings",
            ..
        }
    ));
}

#[test]
fn test_a_dict_literal_is_rejected() {
    // Given
    let input = "{'a': 1}";

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::unsupported("dict and set literals")
    );
}

#[test]
fn test_an_unrecognised_character_is_reported_as_unexpected() {
    // Given
    let input = "status @ 'open'";

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::UnexpectedToken("`@`".to_string())
    );
    assert_eq!(failure.offset, 7);
}

#[test]
fn test_every_token_kind_describes_itself_for_a_diagnostic() {
    // Given
    let all = [
        ident("status"),
        string("open"),
        TokenKind::Int(3),
        TokenKind::True,
        TokenKind::False,
        TokenKind::NoneLit,
        TokenKind::And,
        TokenKind::Or,
        TokenKind::Not,
        TokenKind::In,
        TokenKind::Is,
        TokenKind::EqEq,
        TokenKind::NotEq,
        TokenKind::LParen,
        TokenKind::RParen,
    ];

    // When
    let described: Vec<String> = all.iter().map(TokenKind::describe).collect();

    // Then
    assert!(described.iter().all(|text| !text.is_empty()));
    assert_eq!(described[0], "`status`");
    assert_eq!(described[1], "a string");
}
