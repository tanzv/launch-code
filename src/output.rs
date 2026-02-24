use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;

static JSON_MODE: AtomicBool = AtomicBool::new(false);
static TRACE_TIME_MODE: AtomicBool = AtomicBool::new(false);
static GLOBAL_SESSION_FALLBACK_MODE: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_json_mode(enabled: bool) {
    JSON_MODE.store(enabled, Ordering::Relaxed);
}

pub(crate) fn is_json_mode() -> bool {
    JSON_MODE.load(Ordering::Relaxed)
}

pub(crate) fn set_trace_time_mode(enabled: bool) {
    TRACE_TIME_MODE.store(enabled, Ordering::Relaxed);
}

pub(crate) fn is_trace_time_mode() -> bool {
    TRACE_TIME_MODE.load(Ordering::Relaxed)
}

pub(crate) fn set_global_session_fallback_mode(enabled: bool) {
    GLOBAL_SESSION_FALLBACK_MODE.store(enabled, Ordering::Relaxed);
}

pub(crate) fn is_global_session_fallback_mode() -> bool {
    GLOBAL_SESSION_FALLBACK_MODE.load(Ordering::Relaxed)
}

pub(crate) fn print_trace(message: &str) {
    if is_trace_time_mode() {
        eprintln!("{message}");
    }
}

pub(crate) fn print_message(message: &str) {
    if is_json_mode() {
        let payload = json!({
            "ok": true,
            "message": message
        });
        println!(
            "{}",
            serde_json::to_string(&payload).expect("json output should serialize")
        );
    } else {
        println!("{message}");
    }
}

pub(crate) fn print_lines(lines: &[String]) {
    if is_json_mode() {
        let payload = json!({
            "ok": true,
            "items": lines
        });
        println!(
            "{}",
            serde_json::to_string(&payload).expect("json output should serialize")
        );
    } else {
        if lines.is_empty() {
            println!("no sessions");
            return;
        }
        for line in lines {
            println!("{line}");
        }
    }
}

pub(crate) fn print_json_doc(value: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("json doc should serialize")
    );
}

pub(crate) fn print_text_block(text: &str) {
    if is_json_mode() {
        let payload = json!({
            "ok": true,
            "text": text
        });
        println!(
            "{}",
            serde_json::to_string(&payload).expect("json output should serialize")
        );
    } else {
        print!("{text}");
    }
}
