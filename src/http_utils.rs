use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use launch_code::process::ProcessError;
use serde_json::json;

use crate::error::AppError;

const DEFAULT_MAX_HTTP_JSON_BODY_BYTES: usize = 1_048_576;
static MAX_HTTP_JSON_BODY_BYTES: AtomicUsize = AtomicUsize::new(DEFAULT_MAX_HTTP_JSON_BODY_BYTES);

pub(crate) fn set_http_max_json_body_bytes(value: usize) {
    MAX_HTTP_JSON_BODY_BYTES.store(value, Ordering::Relaxed);
}

pub(crate) fn http_is_authorized(request: &tiny_http::Request, token: &str) -> bool {
    let expected = format!("Bearer {token}");
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Authorization"))
        .map(|header| header.value.as_str())
        .is_some_and(|value| constant_time_eq(value.trim().as_bytes(), expected.as_bytes()))
}

pub(crate) fn http_split_url(url: &str) -> (&str, Option<&str>) {
    match url.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (url, None),
    }
}

pub(crate) fn http_path_segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|item| !item.is_empty()).collect()
}

pub(crate) fn http_query_usize(query: Option<&str>, key: &str) -> Result<Option<usize>, String> {
    let query = match query {
        Some(value) if !value.trim().is_empty() => value,
        _ => return Ok(None),
    };

    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        if k == key {
            let parsed = v.parse::<usize>().map_err(|_| {
                format!("invalid query parameter: {key} should be an unsigned integer")
            })?;
            return Ok(Some(parsed));
        }
    }

    Ok(None)
}

pub(crate) fn http_query_u64(query: Option<&str>, key: &str) -> Result<Option<u64>, String> {
    let query = match query {
        Some(value) if !value.trim().is_empty() => value,
        _ => return Ok(None),
    };

    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        if k == key {
            let parsed = v.parse::<u64>().map_err(|_| {
                format!("invalid query parameter: {key} should be an unsigned integer")
            })?;
            return Ok(Some(parsed));
        }
    }

    Ok(None)
}

pub(crate) fn http_optional_timeout_ms(
    payload: &serde_json::Value,
    key: &str,
    default_ms: u64,
) -> Result<Duration, String> {
    let timeout_ms = match payload.get(key) {
        None => default_ms,
        Some(value) => value
            .as_u64()
            .ok_or_else(|| format!("{key} must be a non-negative integer"))?,
    };
    Ok(Duration::from_millis(timeout_ms.min(60_000)))
}

pub(crate) fn http_json(
    status: tiny_http::StatusCode,
    value: serde_json::Value,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let mut body = serde_json::to_string(&value).unwrap_or_else(|_| "{\"ok\":false}".to_string());
    body.push('\n');
    http_json_response(status, body)
}

pub(crate) fn http_json_error(err: &AppError) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let (status, code) = http_status_for_error(err);
    http_json(
        status,
        json!({"ok": false, "error": code, "message": err.to_string()}),
    )
}

fn http_status_for_error(err: &AppError) -> (tiny_http::StatusCode, &'static str) {
    match err {
        AppError::SessionNotFound(_) => (tiny_http::StatusCode(404), "not_found"),
        AppError::SessionMissingPid(_) => (tiny_http::StatusCode(409), "missing_pid"),
        AppError::SessionMissingDebugMeta(_) => (tiny_http::StatusCode(409), "missing_debug_meta"),
        AppError::SessionMissingLogPath(_) => (tiny_http::StatusCode(409), "missing_log_path"),
        AppError::ProfileNotFound(_) => (tiny_http::StatusCode(404), "profile_not_found"),
        AppError::ProfileBundleVersionUnsupported(_) => (
            tiny_http::StatusCode(422),
            "profile_bundle_version_unsupported",
        ),
        AppError::ProfileValidationFailed(_) => {
            (tiny_http::StatusCode(422), "profile_validation_failed")
        }
        AppError::InvalidEnvPair(_)
        | AppError::InvalidEnvFileLine(_)
        | AppError::InvalidLogRegex(_) => (tiny_http::StatusCode(400), "bad_request"),
        AppError::Process(ProcessError::StopTimeout { .. }) => {
            (tiny_http::StatusCode(409), "stop_timeout")
        }
        AppError::PythonDebugpyUnavailable => (tiny_http::StatusCode(412), "debugpy_unavailable"),
        AppError::Dap(_) => (tiny_http::StatusCode(502), "dap_error"),
        _ => (tiny_http::StatusCode(500), "internal_error"),
    }
}

fn http_json_response(
    status: tiny_http::StatusCode,
    body: String,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let content_type = tiny_http::Header::from_bytes("Content-Type", "application/json")
        .expect("content-type header should be valid");
    tiny_http::Response::from_string(body)
        .with_status_code(status)
        .with_header(content_type)
}

pub(crate) fn http_read_json_body(
    request: &mut tiny_http::Request,
) -> Result<serde_json::Value, HttpReadJsonError> {
    let max_body_bytes = MAX_HTTP_JSON_BODY_BYTES.load(Ordering::Relaxed);
    let mut body = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut total = 0usize;
    let reader = request.as_reader();
    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|err| HttpReadJsonError::Invalid(err.to_string()))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read);
        if total > max_body_bytes {
            return Err(HttpReadJsonError::TooLarge {
                limit_bytes: max_body_bytes,
            });
        }
        body.extend_from_slice(&chunk[..read]);
    }

    let body = String::from_utf8(body)
        .map_err(|err| HttpReadJsonError::Invalid(format!("request body must be UTF-8: {err}")))?;
    if body.trim().is_empty() {
        return Ok(json!({}));
    }

    serde_json::from_str(&body).map_err(|err| HttpReadJsonError::Invalid(err.to_string()))
}

pub(crate) fn http_read_json_object_body(
    request: &mut tiny_http::Request,
) -> Result<serde_json::Value, HttpReadJsonError> {
    let value = http_read_json_body(request)?;
    if value.is_object() {
        return Ok(value);
    }
    Err(HttpReadJsonError::Invalid(
        "request body must be a JSON object".to_string(),
    ))
}

pub(crate) fn http_json_body_error(
    err: HttpReadJsonError,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    match err {
        HttpReadJsonError::TooLarge { limit_bytes } => http_json(
            tiny_http::StatusCode(413),
            json!({
                "ok": false,
                "error": "payload_too_large",
                "message": format!("request body exceeds maximum size of {limit_bytes} bytes"),
            }),
        ),
        HttpReadJsonError::Invalid(message) => http_json(
            tiny_http::StatusCode(400),
            json!({"ok": false, "error": "bad_request", "message": message}),
        ),
    }
}

#[derive(Debug, Clone)]
pub(crate) enum HttpReadJsonError {
    TooLarge { limit_bytes: usize },
    Invalid(String),
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let max_len = left.len().max(right.len());
    let mut diff = left.len() ^ right.len();
    for index in 0..max_len {
        let l = *left.get(index).unwrap_or(&0);
        let r = *right.get(index).unwrap_or(&0);
        diff |= usize::from(l ^ r);
    }
    diff == 0
}
