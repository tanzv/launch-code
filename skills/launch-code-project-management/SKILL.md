---
name: launch-code-project-management
description: Use when using lcode (launch-code) to run, debug, supervise, and troubleshoot project sessions, keep workspace project metadata synchronized, and route commands through global workspace links.
---

# Launch-Code Project Management

## Overview

Use this skill to operate lcode (launch-code) across the daily development cycle:

- Run and debug project code with reproducible commands.
- Debug mode currently supports Python and Node runtimes.
- Direct DAP operations currently support Python runtime only.
- Supervise session lifecycle and recover from failures.
- Inspect logs, process state, and parent/child debug topology.
- Run doctor diagnostics for debug channels with actionable recovery hints.
- Drive lifecycle and debug APIs over HTTP for integrations.
- Maintain workspace metadata in `.launch-code/state.json` (`project_info`).
- Route runtime commands across workspaces using global links (`$HOME/.launch-code/links.json`).

## When to Use

- Need a consistent CLI workflow to launch or debug code during development.
- Need to inspect running sessions and topology to triage issues quickly.
- Need HTTP control-plane operations for scripts or external tooling.
- Need to maintain workspace project metadata used by integrations.
- Need machine-readable JSON output for scripts and CI checks.

Do not use this skill for non-operational project governance topics (roadmaps, staffing, release policy).

## CLI Naming

- Preferred command: `lcode`
- Compatibility command: `launch-code`
- Install locally with `cargo install --path . --force` (both commands are installed).
- Global link metadata is stored at `$HOME/.launch-code/links.json`.
- Runtime write operations default to the current workspace link (`LAUNCH_CODE_HOME` or current directory).
- `lcode list` defaults to global aggregation across all registered links.
- `lcode running` lists only running sessions across the current scope.
- `lcode cleanup` defaults to global cleanup across all registered links.
- `lcode stop --all`, `lcode restart --all`, `lcode suspend --all`, and `lcode resume --all` support batch lifecycle control in scope (`--local`, `--link`, or global default).
- Global non-dry-run batch apply requires explicit `--yes` confirmation; use `--dry-run` for preview.
- Batch lifecycle commands support failure control via `--continue-on-error` and `--max-failures`.
- Session-id commands auto-route by `--id` across links in global scope when `--link` is omitted (`stop/status/inspect/logs/restart/suspend/resume/attach/dap/doctor`).
- `lcode project show` defaults to global project metadata aggregation across links.
- Register links with `lcode link add --name <name> --path <workspace>` and use `--link <name>` to route commands.
- Use `lcode link prune` to clean stale links (`missing_path`, `temporary_empty_path`).
- Set `LCODE_AUTO_PRUNE_VERBOSE=1` to print auto-prune telemetry to stderr in global scan commands.
- `--local` forces workspace-local scope.
- `--global` forces global-link behavior when environment variables would otherwise force local scope.

### Install / Upgrade / Verify

```bash
# Install or upgrade both commands from current repository
cargo install --path . --force

# Verify binaries
which lcode
which launch-code
lcode --help
```

## Core Workflows

### 1. Ad-hoc run/debug session

```bash
lcode start --runtime python --entry app.py --cwd .
lcode start --runtime python --entry app.py --cwd . --foreground --log-mode stdout
lcode start --runtime python --entry app.py --cwd . --foreground --log-mode tee
lcode start --runtime python --entry app.py --cwd . --tail
lcode debug --runtime python --entry app.py --cwd . --host 127.0.0.1 --port 5678 --subprocess true
```

### 1.1 Link bootstrap and routing

```bash
lcode link add --name demo --path /path/to/workspace
lcode link list
lcode link prune --dry-run
lcode link prune
lcode link show --name demo
lcode --link demo list
lcode running
lcode --link demo status --id <session_id>
```

### 2. Launch from `launch.json`

```bash
lcode launch --name "Python Demo" --mode run
lcode launch --name "Python Demo" --mode debug
```

### 3. Saved profile workflow (`config`)

```bash
lcode config save --name "Python Debug" --runtime python --entry app.py --cwd . --mode debug
lcode config validate --name "Python Debug"
lcode config run --name "Python Debug"
lcode config list
lcode config show --name "Python Debug"
```

### 4. Session maintenance and troubleshooting

```bash
lcode list
lcode status --id <session_id>
lcode inspect --id <session_id> --tail 100
lcode logs --id <session_id> --tail 200 --follow
lcode attach --id <session_id>
lcode suspend --id <session_id>
lcode resume --id <session_id>
lcode restart --id <session_id>
lcode stop --id <session_id>
lcode stop --all --status running --yes
lcode restart --all --dry-run --status running
lcode suspend --all --dry-run --status running
lcode resume --all --dry-run --status suspended
lcode suspend --all --status running --max-failures 1
lcode suspend --all --status running --continue-on-error false
lcode doctor debug --id <session_id> --tail 80 --max-events 50 --timeout-ms 1500
lcode daemon --interval-ms 1000
lcode cleanup
lcode cleanup --dry-run --status stopped
lcode --local cleanup
```

Use force-stop only when needed:

```bash
lcode stop --id <session_id> --grace-timeout-ms 100 --force
```

### 5. Project metadata management

```bash
lcode link add --name demo --path /path/to/workspace
lcode link list
lcode link show --name demo
lcode --link demo list
lcode project show
lcode project list
lcode project list --field name --field repository --all
lcode --link demo project show
lcode project set \
  --name "launch-code" \
  --description "IDE-like launch manager" \
  --repository "https://example.com/org/launch-code" \
  --language rust --language python \
  --runtime python \
  --tool debugpy \
  --tag cli
```

Unset selected fields or clear all:

```bash
lcode project unset --field tools --field tags
lcode project unset --field all
lcode project clear
lcode link remove --name demo
```

## Debug Workflow

- Quick DAP inspection:

```bash
lcode dap threads --id <session_id>
lcode dap stack-trace --id <session_id> --thread-id 1 --levels 20
lcode dap scopes --id <session_id> --frame-id 301
lcode dap variables --id <session_id> --variables-reference <ref>
lcode dap events --id <session_id> --max 50 --timeout-ms 1000
```

- Debug control and evaluation:

```bash
lcode dap breakpoints --id <session_id> --path ./app.py --line 12
lcode dap evaluate --id <session_id> --expression "counter + 1" --frame-id 301 --context watch
lcode dap continue --id <session_id> --thread-id 1
lcode dap pause --id <session_id> --thread-id 1
lcode dap next --id <session_id> --thread-id 1
lcode dap step-in --id <session_id> --thread-id 1
lcode dap step-out --id <session_id> --thread-id 1
```

Use `lcode dap adopt-subprocess --id <session_id>` when child-process debug events need to be promoted to managed sessions.

Use `lcode doctor debug --id <session_id>` for one-shot diagnostics that combine session status, inspect output, threads, events, and structured remediation tips.

## HTTP Control Plane

Start server:

```bash
lcode serve --bind 127.0.0.1:9400 --token <TOKEN>
```

Core session endpoints:

- `GET /v1/sessions`
- `GET /v1/sessions/{id}`
- `GET /v1/sessions/{id}/inspect?tail=50`
- `GET /v1/sessions/{id}/debug`
- `POST /v1/sessions/cleanup`
- `POST /v1/sessions/{id}/stop`
- `POST /v1/sessions/{id}/restart`
- `POST /v1/sessions/{id}/suspend`
- `POST /v1/sessions/{id}/resume`

Project metadata endpoints:

- `GET /v1/project`
- `PUT /v1/project`
- `PATCH /v1/project`
- `DELETE /v1/project`

Example calls:

```bash
curl -sS -H "Authorization: Bearer <TOKEN>" \
  http://127.0.0.1:9400/v1/sessions

curl -sS -H "Authorization: Bearer <TOKEN>" \
  http://127.0.0.1:9400/v1/sessions/<session_id>/inspect?tail=100

curl -sS -H "Authorization: Bearer <TOKEN>" \
  http://127.0.0.1:9400/v1/project

curl -sS -X PUT \
  -H "Authorization: Bearer <TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{"name":"launch-code","languages":["rust","python"]}' \
  http://127.0.0.1:9400/v1/project

curl -sS -X DELETE \
  -H "Authorization: Bearer <TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{"fields":["tools","tags"]}' \
  http://127.0.0.1:9400/v1/project
```

## Troubleshooting Matrix

| Symptom | Likely cause | First check | Fast path |
| --- | --- | --- | --- |
| `status` or `inspect` fails for a known id | Session record missing or cleaned | `lcode list` | Re-launch and capture new `session_id` |
| `attach` fails for a running debug session | Debug metadata missing | `lcode inspect --id <session_id> --tail 20` | Start with `debug` mode or use `launch --mode debug` |
| `stop` or `restart` reports state conflict | Concurrent actor changed PID/state | Re-run once after `lcode status --id <session_id>` | Serialize lifecycle actions per session id |
| `stop` times out | Worker ignores graceful signal | `lcode inspect --id <session_id> --tail 100` | `lcode stop --id <session_id> --force --grace-timeout-ms 100` |
| `start` fails with `invalid_start_options` | Incompatible startup flags | Check `--foreground`, `--tail`, and `--log-mode` combination | Use `--tail` only for background mode; use `--log-mode stdout|tee` only with `--foreground` |
| `debug` fails with `unsupported_debug_runtime` | Debug mode is requested for unsupported runtime | Check `--runtime` and profile `mode` | Use Python/Node runtime for debug mode, or switch Rust to run mode |
| `dap` fails with `unsupported_dap_runtime` | DAP operations are requested for non-Python runtime | Check target session runtime from `lcode list` | Use Python debug sessions for `dap` commands, or attach Node sessions from IDE using `attach_vscode` metadata |
| `list` shows `no sessions` unexpectedly | No linked workspace contains sessions, or links are missing | `lcode link list` then `lcode --link <name> list` | Register correct workspace links, or run from the project once to bootstrap link metadata |
| `list` is slow in global mode | Large stale link registry (missing/temp workspaces) | `lcode --json link prune --dry-run` | Run `lcode link prune` and re-run `lcode list` |
| No useful log lines | Wrong filters or log path not present | Remove filters and retry `logs --tail 500` | Use `inspect` `log.text` and simplify regex/include filters |
| Child debug process not visible | Subprocess event not adopted | `lcode dap events --id <session_id> --max 50` | `lcode dap adopt-subprocess --id <session_id>` |
| `doctor debug` reports `D001` | DAP thread request failed | `lcode dap threads --id <session_id>` | Restart session or increase `--timeout-ms` |
| `doctor debug` reports `D002` | DAP event channel not healthy | `lcode dap events --id <session_id> --max 20 --timeout-ms 1500` | Restart session and re-check transport |
| `doctor debug` reports `D003` | Session not running during debug checks | `lcode status --id <session_id>` | `lcode restart --id <session_id>` |
| HTTP request returns `401` | Missing/invalid bearer token | Confirm `Authorization: Bearer <TOKEN>` | Restart `serve` with expected token and retry |
| `cleanup` removes nothing | Target sessions are still `running`/`suspended` | `lcode list --status stopped` and `--status unknown` | Stop sessions first, then run cleanup |
| `--link <name>` fails | Link not registered | `lcode link list` | Add or fix link with `lcode link add --name <name> --path <workspace>` |

## Doctor Diagnostic Codes

`lcode doctor debug --id <session_id> --json` returns `diagnostics[]` entries with `code`, `level`, `summary`, `detail`, and `suggested_actions`.

- `D001`: Thread probe failed (`dap_error`, timeout, disconnected transport).
- `D002`: Event stream probe failed (proxy/channel unavailable).
- `D003`: Session is not running while debug checks fail.
- `D004`: Debugger warning signature detected in inspect log tail.

## Error Code Quick Reference

Use `--json` and inspect `error` in stderr payloads:

- `session_not_found`: Session id does not exist in state.
- `session_missing_pid`: Action requires PID, but record has none.
- `session_missing_debug_meta`: Debug endpoint metadata is absent.
- `session_missing_log_path`: No log file path is available for log operations.
- `session_state_changed`: Concurrent lifecycle mutation was detected.
- `profile_not_found`: Referenced config profile is missing.
- `profile_bundle_version_unsupported`: Imported profile bundle version is unsupported.
- `profile_validation_failed`: Profile content failed validation.
- `invalid_env_pair`: `--env` value is not `KEY=VALUE`.
- `invalid_env_file_line`: Env file contains malformed lines.
- `invalid_log_regex`: `logs` regex or exclude-regex is invalid.
- `invalid_start_options`: Startup flag combination is invalid (`--tail` with `--foreground`, or non-file log mode without foreground).
- `python_debugpy_unavailable`: `debugpy` not importable in selected Python.
- `dap_error`: DAP transport or adapter request failed.
- `http_error`: HTTP client/server side request handling failed.
- `link_not_found`: Requested workspace link does not exist.
- `invalid_link_path`: Provided link path cannot be normalized.

## Validation Rules

- `launch` reads named configurations from `launch.json`; use `start`/`debug` for direct runtime/entry launches.
- `--log-mode stdout|tee` requires `--foreground`.
- `--tail` cannot be combined with `--foreground`.
- Empty update payloads are rejected for `PUT` and `PATCH`.
- List fields must be arrays of strings or `null`.
- `DELETE /v1/project` accepts `{"fields":[...]}`.
- `DELETE /v1/project` with `{}` or `{"all": true}` clears all project metadata.
- State persistence includes `schema_version`; avoid hand-editing unsupported future versions.
- Link registry is separate from runtime state and stored in `$HOME/.launch-code/links.json`.

## Automation and Verification

Use JSON output in automation and assert:

- `ok == true`
- expected `items`/`message`/`project` payloads are present

Relevant regression tests:

```bash
cargo test -q cli_project_info cli_session_topology http_project cli_json_output state_store_persistence
cargo test -q cli_link
```
