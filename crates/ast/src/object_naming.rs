//! Naming helpers shared by every phase that has to agree on what a domain
//! object is *called*: the parser extracting a name from a signature, the
//! analyzer registering it, and the renderer resolving a reference to it.
//!
//! These live here rather than in [`crate::domain_object_body`] because none
//! of them read a `DomainObjectBody` — they work on the raw signature and
//! option text, and are called from outside this crate far more often than
//! from inside it.

use crate::object_type::ObjectType;
use crate::target_name::TargetName;

/// Extracts the referenceable name from a `py:*` domain object signature.
///
/// Takes the text before the first `(` (or the whole string if there is
/// none), then its last whitespace-separated token — e.g. `"foo(bar)"` ->
/// `"foo"`. This also strips an optional base-class list for free, e.g.
/// `"Greeter(Base)"` -> `"Greeter"`. Python-specific: identifiers never
/// start with a pointer sigil, so unlike the C extractor below, there is
/// nothing to strip from the extracted token.
#[must_use]
pub fn extract_python_object_name(signature: &str) -> String {
    let before_parens = signature.split('(').next().unwrap_or(signature).trim();
    before_parens
        .split_whitespace()
        .next_back()
        .unwrap_or(before_parens)
        .to_string()
}

/// Splits one raw `.. option::`/`.. cmdoption::` argument *line* into its
/// comma-separated specs, e.g. `"-c, --compress"` -> `["-c", "--compress"]`.
/// A line with no comma yields a single-element result (`"-m <module>"` ->
/// `["-m <module>"]`). Each piece is trimmed; empty pieces (a stray leading/
/// trailing/doubled comma) are dropped rather than kept as an empty spec.
#[must_use]
pub fn split_option_line_specs(line: &str) -> Vec<String> {
    line.split(',')
        .map(str::trim)
        .filter(|spec| !spec.is_empty())
        .map(str::to_string)
        .collect()
}

/// Extracts the referenceable flag name from one `.. option::` spec,
/// mirroring real Sphinx's `option_desc_re`
/// (`(?:/|--|-|\+)[^\s=]+`): a leading `-`, `--`, `/`, or `+` sigil followed
/// by the run of non-whitespace, non-`=` characters after it — e.g.
/// `"-m <module>"` -> `"-m"`, `"--check-hash-based-pycs default|always|never"`
/// -> `"--check-hash-based-pycs"`, `"--with-wheel-pkg-dir=PATH"` ->
/// `"--with-wheel-pkg-dir"`.
///
/// Falls back to the whole trimmed spec when no sigil matches at all, rather
/// than dropping it — the same "never lose content, degrade to a heuristic
/// instead" convention `extract_c_object_name` follows for a signature its
/// real parser can't handle.
#[must_use]
pub fn extract_option_name(spec: &str) -> String {
    let spec = spec.trim();
    let sigil_len = if spec.starts_with("--") {
        2
    } else if spec.starts_with(['-', '/', '+']) {
        1
    } else {
        return spec.to_string();
    };
    let (sigil, rest) = spec.split_at(sigil_len);
    let flag_body_len = rest
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(rest.len());
    if flag_body_len == 0 {
        // A bare sigil with nothing after it (e.g. just `"-"`) is not a
        // valid flag — fall back to the whole spec rather than returning an
        // empty/meaningless name.
        return spec.to_string();
    }
    format!("{sigil}{}", &rest[..flag_body_len])
}

/// Builds the qualified [`TargetName`] key shared by domain object
/// registration (analyzer) and cross-reference resolution (renderer), so
/// both always agree on the key for the same object.
#[must_use]
pub fn build_domain_object_key(object_type: ObjectType, name: &str) -> TargetName {
    TargetName::new(&format!(
        "{}:{}:{}",
        object_type.domain().as_str(),
        object_type.as_str(),
        name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_object_body::DomainObjectBody;
    use crate::non_empty_vector::NonEmptyVector;
    use crate::object_type::{CObjectType, PyObjectType, StdObjectType};

    #[test]
    fn test_extract_python_object_name_simple_call() {
        // Given
        let signature = "foo(bar)";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_python_object_name_no_parens() {
        // Given
        let signature = "foo";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_python_object_name_no_args() {
        // Given
        let signature = "foo()";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_python_object_name_extra_whitespace() {
        // Given
        let signature = "  foo   (bar)";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_python_object_name_empty_string() {
        // Given
        let signature = "";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "");
    }

    #[test]
    fn test_split_option_line_specs_splits_comma_separated_flags() {
        // Given
        let line = "-c, --compress";

        // When
        let specs = split_option_line_specs(line);

        // Then
        assert_eq!(specs, vec!["-c".to_string(), "--compress".to_string()]);
    }

    #[test]
    fn test_split_option_line_specs_single_flag_with_no_comma() {
        // Given
        let line = "-m <module>";

        // When
        let specs = split_option_line_specs(line);

        // Then
        assert_eq!(specs, vec!["-m <module>".to_string()]);
    }

    #[test]
    fn test_split_option_line_specs_drops_empty_pieces() {
        // Given — a stray trailing comma.
        let line = "-h, --help,";

        // When
        let specs = split_option_line_specs(line);

        // Then
        assert_eq!(specs, vec!["-h".to_string(), "--help".to_string()]);
    }

    #[test]
    fn test_extract_option_name_short_flag() {
        // Given / When / Then
        assert_eq!(extract_option_name("-m <module>"), "-m");
    }

    #[test]
    fn test_extract_option_name_long_flag() {
        // Given / When / Then
        assert_eq!(extract_option_name("--module <module>"), "--module");
    }

    #[test]
    fn test_extract_option_name_long_flag_with_choices_argument() {
        // Given / When / Then
        assert_eq!(
            extract_option_name("--check-hash-based-pycs default|always|never"),
            "--check-hash-based-pycs"
        );
    }

    #[test]
    fn test_extract_option_name_equals_joined_argument() {
        // Given / When / Then
        assert_eq!(
            extract_option_name("--with-wheel-pkg-dir=PATH"),
            "--with-wheel-pkg-dir"
        );
    }

    #[test]
    fn test_extract_option_name_bare_flag_with_no_argument() {
        // Given / When / Then
        assert_eq!(extract_option_name("-h"), "-h");
    }

    #[test]
    fn test_extract_option_name_slash_and_plus_sigils() {
        // Given / When / Then — Windows-style `/` and the rare `+` sigil,
        // both accepted by real Sphinx's `option_desc_re`.
        assert_eq!(extract_option_name("/Wall"), "/Wall");
        assert_eq!(extract_option_name("+x"), "+x");
    }

    #[test]
    fn test_extract_option_name_falls_back_to_whole_spec_when_no_sigil_matches() {
        // Given — malformed: no leading `-`/`--`/`/`/`+`.
        let spec = "not-a-flag";

        // When
        let name = extract_option_name(spec);

        // Then — never lose content, matching `extract_c_object_name`'s
        // fallback philosophy.
        assert_eq!(name, "not-a-flag");
    }

    #[test]
    fn test_extract_option_name_falls_back_for_bare_sigil() {
        // Given — a lone `-` with nothing after it.
        let spec = "-";

        // When
        let name = extract_option_name(spec);

        // Then
        assert_eq!(name, "-");
    }

    #[test]
    fn test_cmdoption_object_type_is_std_cmdoption() {
        // Given
        let obj = DomainObjectBody::StdCmdoption {
            flags: crate::DescriptionFlags::default(),
            signatures: NonEmptyVector::single("-h".to_string()),
            body: vec![],
        };

        // When / Then
        assert_eq!(obj.object_type(), ObjectType::Std(StdObjectType::Cmdoption));
    }

    #[test]
    fn test_cmdoption_names_flattens_comma_separated_specs_across_lines() {
        // Given — one line with two comma-separated specs, and a second,
        // single-spec continuation line.
        let obj = DomainObjectBody::StdCmdoption {
            flags: crate::DescriptionFlags::default(),
            signatures: NonEmptyVector::new(
                "-c, --compress".to_string(),
                vec!["--level <n>".to_string()],
            ),
            body: vec![],
        };

        // When
        let names = obj.names();

        // Then
        assert_eq!(
            names.as_slice(),
            &[
                "-c".to_string(),
                "--compress".to_string(),
                "--level".to_string()
            ]
        );
    }

    #[test]
    fn test_cmdoption_signature_texts_is_one_per_raw_line_not_per_spec() {
        // Given — deliberately asymmetric with `names()` (see its doc
        // comment): one raw line yields two names but one display text.
        let obj = DomainObjectBody::StdCmdoption {
            flags: crate::DescriptionFlags::default(),
            signatures: NonEmptyVector::single("-c, --compress".to_string()),
            body: vec![],
        };

        // When
        let texts = obj.signature_texts();

        // Then
        assert_eq!(texts, vec!["-c, --compress"]);
    }

    #[test]
    fn test_cmdoption_deduce_local_scope_lends_nothing() {
        // Given
        let obj = DomainObjectBody::StdCmdoption {
            flags: crate::DescriptionFlags::default(),
            signatures: NonEmptyVector::single("-h".to_string()),
            body: vec![],
        };

        // When
        let lend = obj.deduce_local_scope(&["irrelevant".to_string()]);

        // Then — options never nest, so nothing is lent to the body.
        assert!(lend.is_empty());
    }

    #[test]
    fn test_cmdoption_has_no_index_options_modeled() {
        // Given
        let obj = DomainObjectBody::StdCmdoption {
            flags: crate::DescriptionFlags::default(),
            signatures: NonEmptyVector::single("-h".to_string()),
            body: vec![],
        };

        // When / Then — real Sphinx's `Cmdoption` has none of these options.
        assert!(!obj.no_index());
        assert!(!obj.no_index_entry());
        assert!(!obj.no_contents_entry());
        assert_eq!(obj.module_override(), None);
    }

    #[test]
    fn test_build_domain_object_key_produces_expected_format() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Function);
        let name = "foo";

        // When
        let key = build_domain_object_key(object_type, name);

        // Then
        assert_eq!(key.as_str(), "py:function:foo");
    }

    #[test]
    fn test_build_domain_object_key_distinguishes_domains() {
        // Given
        let py_type = ObjectType::Py(PyObjectType::Function);
        let c_type = ObjectType::C(CObjectType::Function);
        let name = "foo";

        // When
        let py_key = build_domain_object_key(py_type, name);
        let c_key = build_domain_object_key(c_type, name);

        // Then
        assert_ne!(py_key, c_key);
    }

    #[test]
    fn test_build_domain_object_key_for_module() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Module);
        let name = "mypackage.mymodule";

        // When
        let key = build_domain_object_key(object_type, name);

        // Then
        assert_eq!(key.as_str(), "py:module:mypackage.mymodule");
    }

    #[test]
    fn test_build_domain_object_key_for_data_is_shared_by_data_and_const_roles() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Data);
        let name = "DEFAULT_TIMEOUT";

        // When — both `:py:data:` and `:py:const:` resolve to the same
        // `ObjectType`, so both must build this same key.
        let key = build_domain_object_key(object_type, name);

        // Then — `TargetName` normalizes to lowercase.
        assert_eq!(key.as_str(), "py:data:default_timeout");
    }
}
