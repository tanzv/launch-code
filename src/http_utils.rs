use serde_json::json;

use crate::error::AppError;

pub(crate) fn http_is_authorized(request: &tiny_http::Request, token: &str) -> bool {
    let expected = format!("Bearer {token}");
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Authorization"))
        .map(|header| header.value.as_str())
        .is_some_and(|value| value.trim() == expected)
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
        AppError::ProfileValidationFailed(_) => {
            (tiny_http::StatusCode(422), "profile_validation_failed")
        }
        AppError::InvalidEnvPair(_)
        | AppError::InvalidEnvFileLine(_)
        | AppError::InvalidLogRegex(_) => (tiny_http::StatusCode(400), "bad_request"),
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
) -> Result<serde_json::Value, String> {
    let mut body = String::new();
    request
        .as_reader()
        .read_to_string(&mut body)
        .map_err(|err| err.to_string())?;

    if body.trim().is_empty() {
        return Ok(json!({}));
    }

    serde_json::from_str(&body).map_err(|err| err.to_string())
}
