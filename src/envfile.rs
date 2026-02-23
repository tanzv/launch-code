use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EnvFileError {
    #[error("invalid env file line: {0}")]
    InvalidLine(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub fn parse_env_file_map(path: &Path) -> Result<BTreeMap<String, String>, EnvFileError> {
    let payload = fs::read_to_string(path)?;
    let mut env_map = BTreeMap::new();

    for line in payload.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let trimmed = strip_export_prefix(trimmed);
        let (key, value) = trimmed
            .split_once('=')
            .ok_or_else(|| EnvFileError::InvalidLine(trimmed.to_string()))?;

        let key = key.trim();
        if key.is_empty() {
            continue;
        }

        let value = parse_env_value(value, trimmed)?;
        env_map.insert(key.to_string(), value);
    }

    Ok(env_map)
}

fn strip_export_prefix(raw: &str) -> &str {
    if let Some(rest) = raw.strip_prefix("export") {
        let trimmed = rest.trim_start();
        if trimmed.len() < rest.len() {
            return trimmed;
        }
    }
    raw
}

fn parse_env_value(raw: &str, original_line: &str) -> Result<String, EnvFileError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(String::new());
    }

    if raw.starts_with('"') {
        return parse_double_quoted_value(raw, original_line);
    }
    if raw.starts_with('\'') {
        return parse_single_quoted_value(raw, original_line);
    }

    Ok(parse_unquoted_value(raw))
}

fn parse_double_quoted_value(raw: &str, original_line: &str) -> Result<String, EnvFileError> {
    let mut value = String::new();
    let mut escaped = false;
    let mut closing_index = None;

    for (idx, ch) in raw[1..].char_indices() {
        if escaped {
            value.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                '\'' => '\'',
                '$' => '$',
                other => other,
            });
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        if ch == '"' {
            closing_index = Some(1 + idx);
            break;
        }

        value.push(ch);
    }

    if escaped {
        value.push('\\');
    }

    let closing_index =
        closing_index.ok_or_else(|| EnvFileError::InvalidLine(original_line.to_string()))?;
    let remainder = raw[(closing_index + 1)..].trim();
    if !remainder.is_empty() && !remainder.starts_with('#') {
        return Err(EnvFileError::InvalidLine(original_line.to_string()));
    }

    Ok(value)
}

fn parse_single_quoted_value(raw: &str, original_line: &str) -> Result<String, EnvFileError> {
    let end = raw[1..]
        .find('\'')
        .ok_or_else(|| EnvFileError::InvalidLine(original_line.to_string()))?;
    let value = &raw[1..(1 + end)];
    let remainder = raw[(1 + end + 1)..].trim();
    if !remainder.is_empty() && !remainder.starts_with('#') {
        return Err(EnvFileError::InvalidLine(original_line.to_string()));
    }
    Ok(value.to_string())
}

fn parse_unquoted_value(raw: &str) -> String {
    let mut value = String::new();
    let mut prev_is_whitespace = false;

    for ch in raw.chars() {
        if ch == '#' && prev_is_whitespace {
            break;
        }
        value.push(ch);
        prev_is_whitespace = ch.is_whitespace();
    }

    value.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parse_env_file_map_supports_quotes_and_export_prefix() {
        let tmp = tempdir().expect("temp dir should exist");
        let env_file = tmp.path().join(".env");
        fs::write(
            &env_file,
            "# comment\nA=1\nexport B=\"two\"\nC='three'\nA=override\n",
        )
        .expect("env file should be written");

        let env_map = parse_env_file_map(&env_file).expect("env file should parse");
        assert_eq!(env_map["A"], "override");
        assert_eq!(env_map["B"], "two");
        assert_eq!(env_map["C"], "three");
    }

    #[test]
    fn parse_env_file_map_rejects_invalid_lines() {
        let tmp = tempdir().expect("temp dir should exist");
        let env_file = tmp.path().join("bad.env");
        fs::write(&env_file, "BROKEN\n").expect("env file should be written");

        let err = parse_env_file_map(&env_file).expect_err("env file should fail");
        match err {
            EnvFileError::InvalidLine(line) => assert_eq!(line, "BROKEN"),
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn parse_env_file_map_supports_inline_comments_and_escapes() {
        let tmp = tempdir().expect("temp dir should exist");
        let env_file = tmp.path().join("commented.env");
        fs::write(
            &env_file,
            "A=one # trailing comment\nB=\"two # kept\"\nC='three # kept'\nD=\"line\\nnext\"\n",
        )
        .expect("env file should be written");

        let env_map = parse_env_file_map(&env_file).expect("env file should parse");
        assert_eq!(env_map["A"], "one");
        assert_eq!(env_map["B"], "two # kept");
        assert_eq!(env_map["C"], "three # kept");
        assert_eq!(env_map["D"], "line\nnext");
    }
}
