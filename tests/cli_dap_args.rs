use assert_cmd::cargo::cargo_bin_cmd;

#[test]
fn cli_dap_events_rejects_zero_max() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("events")
        .arg("--id")
        .arg("session-1")
        .arg("--max")
        .arg("0")
        .output()
        .expect("dap events should run");

    assert!(
        !output.status.success(),
        "dap events should reject max below range"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("max must be between 1 and 1000"),
        "error should mention max range"
    );
}

#[test]
fn cli_dap_events_rejects_oversized_max() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("events")
        .arg("--id")
        .arg("session-1")
        .arg("--max")
        .arg("1001")
        .output()
        .expect("dap events should run");

    assert!(
        !output.status.success(),
        "dap events should reject max above range"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("max must be between 1 and 1000"),
        "error should mention max range"
    );
}

#[test]
fn cli_dap_adopt_subprocess_rejects_zero_max_events() {
    let mut cmd = cargo_bin_cmd!("launch-code");
    let output = cmd
        .arg("dap")
        .arg("adopt-subprocess")
        .arg("--id")
        .arg("session-1")
        .arg("--max-events")
        .arg("0")
        .output()
        .expect("dap adopt-subprocess should run");

    assert!(
        !output.status.success(),
        "dap adopt-subprocess should reject max-events below range"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("max-events must be between 1 and 1000"),
        "error should mention max-events range"
    );
}
