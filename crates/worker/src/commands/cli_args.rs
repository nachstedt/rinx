//! Flag-parsing helpers shared by every subcommand's `cmd_*` handler.

use anyhow::{Result, anyhow};

pub(super) fn flag_value(args: &[String], flag: &str) -> Result<String> {
    if let Some(pos) = args.iter().position(|a| a == flag) {
        if pos + 1 < args.len() {
            Ok(args[pos + 1].clone())
        } else {
            Err(anyhow!("Missing value for flag {flag}"))
        }
    } else {
        Err(anyhow!("Missing required flag '{flag}'"))
    }
}

pub(super) fn flag_value_opt(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .and_then(|w| w.get(1).cloned())
}

/// Returns all values that follow `flag` until the next flag (starting with `--`).
pub(super) fn flag_values(args: &[String], flag: &str) -> Result<Vec<String>> {
    let start = args
        .iter()
        .position(|a| a == flag)
        .ok_or_else(|| anyhow!("Missing required flag '{flag}'"))?;

    let values: Vec<String> = args[start + 1..]
        .iter()
        .take_while(|a| !a.starts_with("--"))
        .cloned()
        .collect();

    Ok(values)
}

/// Returns all values that follow `flag` until the next flag, returning empty vector if flag is missing.
pub(super) fn flag_values_opt(args: &[String], flag: &str) -> Vec<String> {
    args.iter()
        .position(|a| a == flag)
        .map(|start| {
            args[start + 1..]
                .iter()
                .take_while(|a| !a.starts_with("--"))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the `arity` values after *every* occurrence of `flag`, one group
/// per occurrence — for a flag a build passes once per declared item, whose
/// values belong together (`--inventory <name> <base-url> <path>`).
///
/// # Errors
///
/// When an occurrence is followed by fewer than `arity` values.
pub(super) fn flag_groups(args: &[String], flag: &str, arity: usize) -> Result<Vec<Vec<String>>> {
    args.iter()
        .enumerate()
        .filter(|(_, arg)| *arg == flag)
        .map(|(position, _)| {
            args.get(position + 1..=position + arity)
                .map(<[String]>::to_vec)
                .ok_or_else(|| anyhow!("Flag {flag} needs {arity} values"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn test_flag_groups_collects_every_occurrence() {
        // Given
        let args = strings(&[
            "--inventory",
            "a",
            "https://a/",
            "a.inv",
            "--output",
            "x",
            "--inventory",
            "b",
            "../b/",
            "b.inv",
        ]);

        // When
        let groups = flag_groups(&args, "--inventory", 3).unwrap();

        // Then
        assert_eq!(
            groups,
            vec![
                strings(&["a", "https://a/", "a.inv"]),
                strings(&["b", "../b/", "b.inv"])
            ]
        );
    }

    #[test]
    fn test_flag_groups_is_empty_without_the_flag() {
        // Given
        let args = strings(&["--output", "x"]);

        // When / Then
        assert!(flag_groups(&args, "--inventory", 3).unwrap().is_empty());
    }

    #[test]
    fn test_flag_groups_refuses_a_short_group() {
        // Given
        let args = strings(&["--inventory", "a", "https://a/"]);

        // When / Then
        assert!(flag_groups(&args, "--inventory", 3).is_err());
    }

    #[test]
    fn test_flag_value_returns_value_when_flag_exists() {
        // Given
        let args = vec!["--input".to_string(), "a.rst".to_string()];

        // When
        let result = flag_value(&args, "--input");

        // Then
        assert_eq!(result.unwrap(), "a.rst");
    }

    #[test]
    fn test_flag_value_returns_error_when_flag_is_missing() {
        // Given
        let args = vec!["--input".to_string(), "a.rst".to_string()];

        // When
        let result = flag_value(&args, "--output");

        // Then
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Missing required flag '--output'"
        );
    }

    #[test]
    fn test_flag_value_returns_error_when_value_is_missing() {
        // Given
        let args = vec!["--input".to_string()];

        // When
        let result = flag_value(&args, "--input");

        // Then
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Missing value for flag --input"
        );
    }

    #[test]
    fn test_flag_value_opt_returns_some_when_flag_exists() {
        // Given
        let args = vec!["--input".to_string(), "a.rst".to_string()];

        // When
        let result = flag_value_opt(&args, "--input");

        // Then
        assert_eq!(result.unwrap(), "a.rst");
    }

    #[test]
    fn test_flag_value_opt_returns_none_when_flag_is_missing() {
        // Given
        let args = vec!["--input".to_string(), "a.rst".to_string()];

        // When
        let result = flag_value_opt(&args, "--output");

        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_flag_value_opt_returns_none_when_flag_at_end_without_value() {
        // Given
        let args = vec![
            "--input".to_string(),
            "a.rst".to_string(),
            "--output".to_string(),
        ];

        // When
        let result = flag_value_opt(&args, "--output");

        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_flag_value_opt_returns_none_when_args_is_empty() {
        // Given
        let args: Vec<String> = vec![];

        // When
        let result = flag_value_opt(&args, "--output");

        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_flag_value_opt_returns_first_value_when_flag_appears_multiple_times() {
        // Given
        let args = vec![
            "--input".to_string(),
            "a.rst".to_string(),
            "--input".to_string(),
            "b.rst".to_string(),
        ];

        // When
        let result = flag_value_opt(&args, "--input");

        // Then
        assert_eq!(result.unwrap(), "a.rst");
    }

    #[test]
    fn test_flag_values_returns_list_of_values() {
        // Given
        let args = vec![
            "--inputs".to_string(),
            "a.ast".to_string(),
            "b.ast".to_string(),
            "--output".to_string(),
            "c.idx".to_string(),
        ];

        // When
        let result = flag_values(&args, "--inputs");

        // Then
        assert_eq!(result.unwrap(), vec!["a.ast", "b.ast"]);
    }

    #[test]
    fn test_flag_values_returns_empty_list_when_no_values_follow_flag() {
        // Given
        let args = vec![
            "--inputs".to_string(),
            "--output".to_string(),
            "c.idx".to_string(),
        ];

        // When
        let result = flag_values(&args, "--inputs");

        // Then
        let expected: Vec<String> = vec![];
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_flag_values_returns_error_when_flag_is_missing() {
        // Given
        let args = vec!["--output".to_string(), "c.idx".to_string()];

        // When
        let result = flag_values(&args, "--inputs");

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_flag_values_opt_returns_list_of_values() {
        // Given
        let args = vec![
            "--inputs".to_string(),
            "a.ast".to_string(),
            "b.ast".to_string(),
            "--output".to_string(),
            "c.idx".to_string(),
        ];

        // When
        let result = flag_values_opt(&args, "--inputs");

        // Then
        assert_eq!(result, vec!["a.ast", "b.ast"]);
    }

    #[test]
    fn test_flag_values_opt_returns_empty_list_when_no_values_follow_flag() {
        // Given
        let args = vec![
            "--inputs".to_string(),
            "--output".to_string(),
            "c.idx".to_string(),
        ];

        // When
        let result = flag_values_opt(&args, "--inputs");

        // Then
        let expected: Vec<String> = vec![];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_flag_values_opt_returns_empty_list_when_flag_is_missing() {
        // Given
        let args = vec!["--output".to_string(), "c.idx".to_string()];

        // When
        let result = flag_values_opt(&args, "--inputs");

        // Then
        let expected: Vec<String> = vec![];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_flag_values_opt_returns_empty_list_when_args_is_empty() {
        // Given
        let args: Vec<String> = vec![];

        // When
        let result = flag_values_opt(&args, "--inputs");

        // Then
        let expected: Vec<String> = vec![];
        assert_eq!(result, expected);
    }
}
