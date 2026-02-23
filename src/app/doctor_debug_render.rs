use launch_code::model::SessionRecord;

use super::doctor_debug_diagnostics::status_label;

pub(super) fn print_text_summary(
    session: &SessionRecord,
    adapter: &serde_json::Value,
    threads: &serde_json::Value,
    events: &serde_json::Value,
    diagnostics: &[serde_json::Value],
) {
    let pid = session
        .pid
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string());
    println!(
        "doctor_debug session_id={} status={} pid={}",
        session.id,
        status_label(&session.status),
        pid
    );

    if adapter["ok"].as_bool().unwrap_or(false) {
        let source = adapter["source"].as_str().unwrap_or("unknown");
        let command = adapter["command"].as_str().unwrap_or("-");
        println!("adapter_ok=true source={source} command={command}");
    } else {
        let source = adapter["source"].as_str().unwrap_or("unknown");
        let message = adapter["message"].as_str().unwrap_or("unknown");
        println!("adapter_ok=false source={source} message={message}");
    }

    if threads["ok"].as_bool().unwrap_or(false) {
        let count = threads["response"]["body"]["threads"]
            .as_array()
            .map(|items| items.len())
            .unwrap_or(0);
        println!("threads_ok=true thread_count={count}");
    } else {
        let message = threads["message"].as_str().unwrap_or("unknown");
        println!("threads_ok=false message={message}");
    }

    if events["ok"].as_bool().unwrap_or(false) {
        let count = events["count"].as_u64().unwrap_or(0);
        println!("events_ok=true count={count}");
    } else {
        let message = events["message"].as_str().unwrap_or("unknown");
        println!("events_ok=false message={message}");
    }

    if diagnostics.is_empty() {
        println!("diagnostics=none");
        return;
    }

    println!("diagnostics_count={}", diagnostics.len());
    for item in diagnostics {
        let code = item["code"].as_str().unwrap_or("unknown");
        let level = item["level"].as_str().unwrap_or("unknown");
        let summary = item["summary"].as_str().unwrap_or("unknown");
        println!("diagnostic code={code} level={level} summary={summary}");
    }
}
