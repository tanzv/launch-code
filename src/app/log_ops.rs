use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::thread;
use std::time::Duration;

use launch_code::model::unix_timestamp_secs;
use launch_code::process::is_process_alive;
use launch_code::state::StateStore;
use regex::{Regex, RegexBuilder};

use crate::cli::LogsArgs;
use crate::error::AppError;
use crate::output;

pub(in crate::app) const MAX_LOG_TAIL_LINES: usize = 5000;

pub(super) fn handle_logs(store: &StateStore, args: &LogsArgs) -> Result<(), AppError> {
    let Some(session_id) = args.resolved_id() else {
        return Ok(());
    };
    let filter = LogFilter::new(
        args.contains.clone(),
        args.exclude.clone(),
        args.regex.clone(),
        args.exclude_regex.clone(),
        args.ignore_case,
    )?;
    let session_id = super::session_api::resolve_session_id(store, session_id)?;
    let (log_path, pid) = store.update::<_, _, AppError>(|state| {
        let now = unix_timestamp_secs();
        let session = super::find_session_mut(state, &session_id)?;
        super::reconcile_session(store, session, now)?;
        let log_path = session
            .log_path
            .clone()
            .ok_or_else(|| AppError::SessionMissingLogPath(session.id.clone()))?;
        Ok((log_path, session.pid))
    })?;

    let mut content = read_log_tail(Some(&log_path), args.tail).unwrap_or_default();
    content = filter.filter_block(&content);
    if !content.is_empty() {
        output::print_text_block(&format!("{content}\n"));
    }

    if !args.follow {
        return Ok(());
    }

    follow_log_until_exit(
        store,
        &session_id,
        Path::new(&log_path),
        pid,
        Duration::from_millis(args.poll_ms.max(10)),
        &filter,
    )?;
    Ok(())
}

pub(in crate::app) fn read_log_tail(
    path: Option<&str>,
    max_lines: usize,
) -> Result<String, AppError> {
    let max_lines = max_lines.min(MAX_LOG_TAIL_LINES);
    if max_lines == 0 {
        return Ok(String::new());
    }

    let path = match path {
        Some(value) if !value.trim().is_empty() => value,
        _ => return Ok(String::new()),
    };

    let mut file = fs::File::open(path)?;
    let mut position = file.metadata()?.len();
    if position == 0 {
        return Ok(String::new());
    }

    let mut newline_count = 0usize;
    let mut chunks: Vec<Vec<u8>> = Vec::new();
    const CHUNK_SIZE_BYTES: u64 = 8192;

    while position > 0 && newline_count <= max_lines {
        let read_size = CHUNK_SIZE_BYTES.min(position);
        position -= read_size;
        file.seek(SeekFrom::Start(position))?;

        let mut chunk = vec![0u8; read_size as usize];
        file.read_exact(&mut chunk)?;
        newline_count =
            newline_count.saturating_add(chunk.iter().filter(|byte| **byte == b'\n').count());
        chunks.push(chunk);
    }

    chunks.reverse();
    let total_size = chunks
        .iter()
        .fold(0usize, |acc, chunk| acc.saturating_add(chunk.len()));
    let mut payload = Vec::with_capacity(total_size);
    for chunk in chunks {
        payload.extend_from_slice(&chunk);
    }

    let payload = String::from_utf8_lossy(&payload);
    let mut lines: Vec<&str> = payload.lines().collect();
    if lines.len() > max_lines {
        lines = lines.split_off(lines.len() - max_lines);
    }
    Ok(lines.join("\n"))
}

#[derive(Debug, Clone)]
struct LogFilter {
    contains: Vec<String>,
    exclude: Vec<String>,
    regex: Option<Regex>,
    exclude_regex: Option<Regex>,
    ignore_case: bool,
}

impl LogFilter {
    fn new(
        contains: Vec<String>,
        exclude: Vec<String>,
        regex: Option<String>,
        exclude_regex: Option<String>,
        ignore_case: bool,
    ) -> Result<Self, AppError> {
        let contains = if ignore_case {
            contains
                .into_iter()
                .map(|value| value.to_lowercase())
                .collect()
        } else {
            contains
        };
        let exclude = if ignore_case {
            exclude
                .into_iter()
                .map(|value| value.to_lowercase())
                .collect()
        } else {
            exclude
        };
        let regex = match regex {
            Some(pattern) => Some(
                RegexBuilder::new(&pattern)
                    .case_insensitive(ignore_case)
                    .build()
                    .map_err(|err| AppError::InvalidLogRegex(err.to_string()))?,
            ),
            None => None,
        };
        let exclude_regex = match exclude_regex {
            Some(pattern) => Some(
                RegexBuilder::new(&pattern)
                    .case_insensitive(ignore_case)
                    .build()
                    .map_err(|err| AppError::InvalidLogRegex(err.to_string()))?,
            ),
            None => None,
        };
        Ok(Self {
            contains,
            exclude,
            regex,
            exclude_regex,
            ignore_case,
        })
    }

    fn is_enabled(&self) -> bool {
        !self.contains.is_empty()
            || !self.exclude.is_empty()
            || self.regex.is_some()
            || self.exclude_regex.is_some()
    }

    fn matches_line(&self, line: &str) -> bool {
        let lowered;
        let source = if self.ignore_case {
            lowered = line.to_lowercase();
            lowered.as_str()
        } else {
            line
        };

        if !self.contains.is_empty()
            && !self
                .contains
                .iter()
                .any(|pattern| source.contains(pattern.as_str()))
        {
            return false;
        }

        if let Some(regex) = &self.regex {
            if !regex.is_match(line) {
                return false;
            }
        }

        if self
            .exclude
            .iter()
            .any(|pattern| source.contains(pattern.as_str()))
        {
            return false;
        }

        if let Some(exclude_regex) = &self.exclude_regex {
            if exclude_regex.is_match(line) {
                return false;
            }
        }

        true
    }

    fn filter_block(&self, block: &str) -> String {
        if !self.is_enabled() {
            return block.to_string();
        }

        block
            .lines()
            .filter(|line| self.matches_line(line))
            .collect::<Vec<&str>>()
            .join("\n")
    }
}

fn follow_log_until_exit(
    store: &StateStore,
    session_id: &str,
    log_path: &Path,
    initial_pid: Option<u32>,
    poll_interval: Duration,
    filter: &LogFilter,
) -> Result<(), AppError> {
    let mut offset = fs::metadata(log_path).map(|meta| meta.len()).unwrap_or(0);
    let mut idle_after_exit = false;
    let mut partial_line = String::new();

    loop {
        if let Ok(mut file) = fs::File::open(log_path) {
            file.seek(SeekFrom::Start(offset))?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            if !buf.is_empty() {
                offset += u64::try_from(buf.len()).unwrap_or(0);
                let text = String::from_utf8_lossy(&buf).to_string();
                if filter.is_enabled() {
                    partial_line.push_str(&text);
                    let ends_with_newline = partial_line.ends_with('\n');
                    let mut chunks: Vec<&str> = partial_line.split('\n').collect();
                    let remainder = if ends_with_newline {
                        String::new()
                    } else {
                        chunks.pop().unwrap_or_default().to_string()
                    };
                    let filtered = chunks
                        .into_iter()
                        .filter(|line| !line.is_empty() && filter.matches_line(line))
                        .collect::<Vec<&str>>()
                        .join("\n");
                    if !filtered.is_empty() {
                        output::print_text_block(&format!("{filtered}\n"));
                    }
                    partial_line = remainder;
                } else {
                    output::print_text_block(&text);
                }
                idle_after_exit = false;
            }
        }

        let alive = {
            let state = store.load()?;
            let pid = state
                .sessions
                .get(session_id)
                .and_then(|session| session.pid)
                .or(initial_pid);
            pid.map(is_process_alive).unwrap_or(false)
        };

        if !alive {
            if idle_after_exit {
                break;
            }
            idle_after_exit = true;
        }

        thread::sleep(poll_interval);
    }

    if filter.is_enabled() && !partial_line.is_empty() && filter.matches_line(&partial_line) {
        output::print_text_block(&format!("{partial_line}\n"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::read_log_tail;

    #[test]
    fn read_log_tail_returns_last_lines_without_trailing_newline() {
        let mut file = NamedTempFile::new().expect("temp file should be created");
        writeln!(file, "line-1").expect("write line-1");
        writeln!(file, "line-2").expect("write line-2");
        writeln!(file, "line-3").expect("write line-3");
        write!(file, "line-4").expect("write line-4");

        let path = file.path().to_string_lossy().to_string();
        let tail = read_log_tail(Some(&path), 2).expect("read tail should succeed");
        assert_eq!(tail, "line-3\nline-4");
    }

    #[test]
    fn read_log_tail_returns_all_lines_when_max_exceeds_file_length() {
        let mut file = NamedTempFile::new().expect("temp file should be created");
        writeln!(file, "line-1").expect("write line-1");
        writeln!(file, "line-2").expect("write line-2");
        write!(file, "line-3").expect("write line-3");

        let path = file.path().to_string_lossy().to_string();
        let tail = read_log_tail(Some(&path), 50).expect("read tail should succeed");
        assert_eq!(tail, "line-1\nline-2\nline-3");
    }

    #[test]
    fn read_log_tail_clamps_very_large_tail_requests() {
        let mut file = NamedTempFile::new().expect("temp file should be created");
        for line in 0..6000 {
            writeln!(file, "line-{line}").expect("write line");
        }

        let path = file.path().to_string_lossy().to_string();
        let tail = read_log_tail(Some(&path), 6000).expect("read tail should succeed");
        let lines: Vec<&str> = tail.lines().collect();
        assert_eq!(lines.len(), 5000);
        assert_eq!(lines.first().copied(), Some("line-1000"));
        assert_eq!(lines.last().copied(), Some("line-5999"));
    }
}
