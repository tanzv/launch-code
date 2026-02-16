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

        let trimmed = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        let (key, value) = trimmed
            .split_once('=')
            .ok_or_else(|| EnvFileError::InvalidLine(trimmed.to_string()))?;

        let key = key.trim();
        if key.is_empty() {
            continue;
        }

        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
            .or_else(|| {
                value
                    .strip_prefix('\'')
                    .and_then(|inner| inner.strip_suffix('\''))
            })
            .unwrap_or(value);

        env_map.insert(key.to_string(), value.to_string());
    }

    Ok(env_map)
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
}
