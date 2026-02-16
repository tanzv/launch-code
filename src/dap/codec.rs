use std::io::{BufRead, Write};

use crate::error::AppError;

const MAX_DAP_CONTENT_LENGTH: usize = 16 * 1024 * 1024;

pub(super) fn write_message<W: Write>(
    writer: &mut W,
    msg: &serde_json::Value,
) -> Result<(), AppError> {
    let payload = serde_json::to_vec(msg).map_err(|err| AppError::Dap(err.to_string()))?;
    let header = format!("Content-Length: {}\r\n\r\n", payload.len());
    writer
        .write_all(header.as_bytes())
        .map_err(|err| AppError::Dap(err.to_string()))?;
    writer
        .write_all(&payload)
        .map_err(|err| AppError::Dap(err.to_string()))?;
    writer
        .flush()
        .map_err(|err| AppError::Dap(err.to_string()))?;
    Ok(())
}

pub(super) fn read_message<R: BufRead>(reader: &mut R) -> Result<serde_json::Value, AppError> {
    let mut content_len: Option<usize> = None;
    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|err| AppError::Dap(err.to_string()))?;
        if bytes == 0 {
            return Err(AppError::Dap(
                "unexpected eof while reading headers".to_string(),
            ));
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }

        let lower = trimmed.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            let parsed = rest
                .trim()
                .parse::<usize>()
                .map_err(|err| AppError::Dap(err.to_string()))?;
            if parsed > MAX_DAP_CONTENT_LENGTH {
                return Err(AppError::Dap(format!(
                    "content-length too large: {parsed} > {MAX_DAP_CONTENT_LENGTH}"
                )));
            }
            content_len = Some(parsed);
        }
    }

    let len =
        content_len.ok_or_else(|| AppError::Dap("missing content-length header".to_string()))?;
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .map_err(|err| AppError::Dap(err.to_string()))?;
    serde_json::from_slice(&buf).map_err(|err| AppError::Dap(err.to_string()))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use serde_json::json;

    use super::{read_message, write_message};

    #[test]
    fn read_message_rejects_too_large_content_length() {
        let payload = b"Content-Length: 16777217\r\n\r\n";
        let mut reader = Cursor::new(payload.as_slice());
        let err = read_message(&mut reader).expect_err("message should fail");
        assert!(
            err.to_string().contains("content-length too large"),
            "error should mention content-length guard"
        );
    }

    #[test]
    fn read_write_roundtrip_works() {
        let message = json!({
            "seq": 1,
            "type": "request",
            "command": "threads",
            "arguments": {}
        });
        let mut bytes = Vec::<u8>::new();
        write_message(&mut bytes, &message).expect("message should be encoded");

        let mut reader = Cursor::new(bytes);
        let decoded = read_message(&mut reader).expect("message should be decoded");
        assert_eq!(decoded, message);
    }
}
