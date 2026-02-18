# Launch Manager MVP Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use executing-plans to implement this plan task-by-task.

**Goal:** Build a Rust CLI MVP that provides IDE-like run management for multi-language projects with Python runtime support first.

**Architecture:** Use a command-driven binary with modular runtime adapters and a JSON state store under `.launch-code/state.json`. Each launch session stores command spec, PID, lifecycle status, and log path; Unix signals are used for suspend/resume/stop.

**Tech Stack:** Rust 2021, `clap`, `serde`, `serde_json`, `thiserror`, `uuid`, `tempfile`, `assert_cmd`, `predicates`, `libc` (unix only)

---

### Task 1: Initialize project skeleton

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/cli.rs`
- Create: `src/error.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn binary_shows_help() {
    let mut cmd = assert_cmd::Command::cargo_bin("lcode").unwrap();
    cmd.arg("--help");
    cmd.assert().success();
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test binary_shows_help -- --nocapture`
Expected: FAIL because binary and CLI wiring do not exist yet.

**Step 3: Write minimal implementation**

- Add `clap` dependency and command structure.
- Wire `main()` to parse CLI.

**Step 4: Run test to verify it passes**

Run: `cargo test binary_shows_help -- --nocapture`
Expected: PASS.

**Step 5: Commit**

```bash
git add Cargo.toml src/main.rs src/cli.rs src/error.rs
git commit -m "feat(cli): bootstrap command skeleton"
```

### Task 2: Add runtime adapters and command resolution

**Files:**
- Create: `src/runtime/mod.rs`
- Create: `src/runtime/python.rs`
- Create: `src/runtime/node.rs`
- Create: `src/runtime/rust.rs`
- Test: `tests/runtime_command_building.rs`

**Step 1: Write the failing test**

- Assert Python run builds `python <entry> ...`
- Assert Python debug builds `python -m debugpy --listen ... --wait-for-client <entry> ...`

**Step 2: Run test to verify it fails**

Run: `cargo test runtime_command_building -- --nocapture`
Expected: FAIL because adapter module is missing.

**Step 3: Write minimal implementation**

- Implement runtime resolution and command builders for Python/Node/Rust.

**Step 4: Run test to verify it passes**

Run: `cargo test runtime_command_building -- --nocapture`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/runtime tests/runtime_command_building.rs
git commit -m "feat(runtime): add language adapter command builders"
```

### Task 3: Add state store and session model

**Files:**
- Create: `src/model.rs`
- Create: `src/state.rs`
- Test: `tests/state_store_persistence.rs`

**Step 1: Write the failing test**

- Save state with one session and reload from disk.
- Assert fields are preserved.

**Step 2: Run test to verify it fails**

Run: `cargo test state_store_persistence -- --nocapture`
Expected: FAIL due to missing state module.

**Step 3: Write minimal implementation**

- Implement `StateStore::load/save` and atomic save with temp file.

**Step 4: Run test to verify it passes**

Run: `cargo test state_store_persistence -- --nocapture`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/model.rs src/state.rs tests/state_store_persistence.rs
git commit -m "feat(state): persist launch sessions in workspace store"
```

### Task 4: Implement process lifecycle operations

**Files:**
- Create: `src/process.rs`
- Modify: `src/main.rs`
- Test: `tests/process_lifecycle.rs`

**Step 1: Write the failing test**

- Start a long-running process (`sleep` equivalent) using internal helper.
- Verify alive, suspend, resume, stop transitions.

**Step 2: Run test to verify it fails**

Run: `cargo test process_lifecycle -- --nocapture`
Expected: FAIL with missing process helpers.

**Step 3: Write minimal implementation**

- Spawn detached process and route output to `.launch-code/logs/<id>.log`.
- Add Unix signal helpers for stop/suspend/resume/status.

**Step 4: Run test to verify it passes**

Run: `cargo test process_lifecycle -- --nocapture`
Expected: PASS on Unix.

**Step 5: Commit**

```bash
git add src/process.rs src/main.rs tests/process_lifecycle.rs
git commit -m "feat(process): implement lifecycle control using unix signals"
```

### Task 5: Integrate CLI subcommands for run management

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Create: `tests/cli_workflow.rs`

**Step 1: Write the failing test**

- `start` creates session and prints session id.
- `status` returns running state.
- `stop` marks session stopped.

**Step 2: Run test to verify it fails**

Run: `cargo test cli_workflow -- --nocapture`
Expected: FAIL because command handlers are incomplete.

**Step 3: Write minimal implementation**

- Implement `start/debug/suspend/resume/stop/restart/status/list`.
- Persist state transitions.

**Step 4: Run test to verify it passes**

Run: `cargo test cli_workflow -- --nocapture`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/cli.rs src/main.rs tests/cli_workflow.rs
git commit -m "feat(cli): add ide-like run lifecycle commands"
```

### Task 6: Final verification and usage docs

**Files:**
- Create: `README.md`

**Step 1: Run full test suite**

Run: `cargo test --all -- --nocapture`
Expected: PASS.

**Step 2: Run lint**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: PASS.

**Step 3: Write usage guide**

- Document lifecycle commands and Python debug example.

**Step 4: Commit**

```bash
git add README.md
git commit -m "docs: document launch manager usage and examples"
```
