use std::collections::BTreeMap;
use std::fs;
use std::thread;
use std::time::Duration;

use launch_code::process::{
    is_process_alive, resume_process, spawn_process, stop_process, suspend_process,
};
use tempfile::tempdir;

#[test]
fn process_can_be_spawned_suspended_resumed_and_stopped() {
    let tmp = tempdir().expect("temp dir should exist");
    let log_path = tmp.path().join("lifecycle.log");
    let command = vec!["sleep".to_string(), "30".to_string()];

    let pid = spawn_process(&command, tmp.path(), &BTreeMap::new(), &log_path)
        .expect("process should start");
    thread::sleep(Duration::from_millis(100));
    assert!(
        is_process_alive(pid),
        "process should be running after start"
    );

    suspend_process(pid).expect("suspend should succeed");
    thread::sleep(Duration::from_millis(100));
    assert!(is_process_alive(pid), "suspended process still exists");

    resume_process(pid).expect("resume should succeed");
    thread::sleep(Duration::from_millis(100));
    assert!(
        is_process_alive(pid),
        "process should continue after resume"
    );

    stop_process(pid).expect("stop should succeed");

    for _ in 0..20 {
        if !is_process_alive(pid) {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }

    panic!("process should stop after stop command");
}

#[cfg(unix)]
#[test]
fn stop_process_terminates_child_processes_in_same_group() {
    let tmp = tempdir().expect("temp dir should exist");
    let log_path = tmp.path().join("group-stop.log");
    let child_pid_path = tmp.path().join("child.pid");
    let command = vec![
        "sh".to_string(),
        "-c".to_string(),
        "sleep 30 & echo $! > child.pid; wait".to_string(),
    ];

    let pid = spawn_process(&command, tmp.path(), &BTreeMap::new(), &log_path)
        .expect("process should start");

    let child_pid = wait_child_pid_file(&child_pid_path);
    assert!(is_process_alive(pid), "parent should be alive");
    assert!(is_process_alive(child_pid), "child should be alive");

    stop_process(pid).expect("stop should succeed");

    for _ in 0..30 {
        if !is_process_alive(pid) && !is_process_alive(child_pid) {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }

    // Best-effort cleanup to avoid leaking child process when assertion fails.
    unsafe {
        let _ = libc::kill(child_pid as i32, libc::SIGKILL);
    }
    panic!("stop should terminate both parent and child process");
}

#[cfg(unix)]
fn wait_child_pid_file(path: &std::path::Path) -> u32 {
    for _ in 0..30 {
        if let Ok(raw) = fs::read_to_string(path) {
            if let Ok(pid) = raw.trim().parse::<u32>() {
                return pid;
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("child pid file should be created");
}
