# launch-code

`launch-code` is a Rust CLI that provides IDE-like run management for local development workflows.  
Recommended command: `lcode` (compatibility command: `launch-code`).

## Features

- Multi-runtime launch adapters: Python, Node, Rust
- Run and debug modes (`start`, `debug`)
- Workspace session persistence (`.launch-code/state.json`)
- Atomic state updates for concurrent CLI/HTTP writers (multi-process safe persistence)
- Process lifecycle controls (`stop`, `restart`, `suspend`, `resume`)
- Batch lifecycle controls (`stop --all`, `restart --all`, `suspend --all`, `resume --all`) with scope-aware filtering
- Batch failure controls (`--continue-on-error`, `--max-failures`) for `--all` lifecycle commands
- Global non-dry-run batch lifecycle apply requires explicit `--yes` confirmation
- Session state cleanup for stale records (`cleanup`, global-by-default)
- Graceful/forced stop strategy (`stop --grace-timeout-ms`, optional `--force`)
- Managed sessions with automatic restart after worker exit
- Reconciliation daemon (`daemon`)
- HTTP control plane (`serve`) for status, lifecycle commands, and debug proxying
- VS Code `launch.json` compatibility (`launch` command)
- Saved profile management (`config save/list/show/run/delete`)
- Workspace project metadata management (`project show/list/set/unset/clear`)
- Global workspace link registry (`link add/list/show/remove/prune`) with `--link <name>` routing
- Default global session listing (`lcode list`) aggregated across registered links
- `preLaunchTask` and `postStopTask` hooks for launch configurations
- Debug port conflict fallback with session metadata output
- Structured CLI output (`--json`) with stable machine-readable error codes
- Doctor debug diagnostics with structured remediation codes (`D001`-`D004`)

## Install and Build

```bash
cargo build
```

Install to local cargo bin path:

```bash
cargo install --path . --force
```

Installed commands:

- `lcode` (recommended short command)
- `launch-code` (compatibility command)

State scope:

- Global link metadata is stored at `$HOME/.launch-code/links.json`
- Runtime writes (start/debug/launch/config/project/session actions) default to the current workspace link
- `lcode list` defaults to global aggregation across all registered links (unless `--local`/`--link` is used)
- `lcode running` is a shortcut for listing only running sessions in the current scope (compact view by default)
- `lcode list` supports display options `--format <table|compact|wide|id>` (aliases: `default/short/debug`), `--compact`, `--quiet/-q`, `--no-trunc`, `--short-id-len`, and `--no-headers`
- `lcode running` supports display options `--format <table|compact|wide|id>` (aliases: `default/short/debug`), `--wide`, `--quiet/-q`, `--no-trunc`, `--short-id-len`, and `--no-headers`, plus runtime/name filters
- `lcode cleanup` defaults to global cleanup across registered links (unless `--local`/`--link` is used)
- Global `list`/`running`/`cleanup`/`project show` can auto-prune stale links when link registry is very large
- Session-id commands (for example `stop`, `status`, `inspect`, `logs`, `restart`, `suspend`, `resume`, `attach`, `dap`, `doctor`) auto-route by `--id` across links when global scope is active and `--link` is omitted
- Session-id lifecycle and diagnostics commands support unique short-id prefixes in addition to full id (for example `lcode status 249b103f`)
- Session lookup cache is stored at `$HOME/.launch-code/session-index.json` to accelerate repeated cross-link `--id` routing
- `lcode project show` defaults to global project metadata aggregation across links
- Use `lcode link add --name <name> --path <workspace>` to register a workspace explicitly
- Use `lcode link prune` to clean stale links (missing paths and temporary empty workspaces)
- If global listing becomes slow, run `lcode link prune --dry-run` then `lcode link prune`
- Set `LCODE_AUTO_PRUNE_VERBOSE=1` to emit auto-prune telemetry to stderr during global scans
- Use `--link <name>` to route commands to one linked workspace
- Use `--local` to force current workspace scope (`LAUNCH_CODE_HOME` or current directory)

## Documentation

- `docs/python-debug-manual.md`: End-to-end Python debug workflow for CLI and HTTP.
- `docs/examples/python-debug-demo/app.py`: Minimal Python script for breakpoint and stepping demos.

## Python Debug Requirements

Python debug mode uses `debugpy`.
Node debug mode is supported for process startup and endpoint metadata.
Rust debug mode is currently not supported.

```bash
python -m pip install debugpy
```

Optional interpreter override for run/debug sessions:

```bash
lcode debug --runtime python --entry app.py --cwd . --env PYTHON_BIN=/path/to/python
lcode debug --runtime python --entry app.py --cwd . --subprocess true
```

## CLI Commands

```bash
lcode start --runtime python --entry app.py --cwd .
lcode start --runtime python --entry app.py --cwd . --env-file ./.env.base --env-file ./.env.local --env API_URL=http://127.0.0.1:9000
lcode start --runtime python --entry app.py --cwd . --foreground --log-mode stdout
lcode start --runtime python --entry app.py --cwd . --foreground --log-mode tee
lcode start --runtime python --entry app.py --cwd . --tail
lcode debug --runtime python --entry app.py --cwd . --host 127.0.0.1 --port 5678 --subprocess true
lcode debug --runtime python --entry app.py --cwd . --env-file ./.env.base --env DEBUG=1
lcode debug --runtime node --entry app.js --cwd . --host 127.0.0.1 --port 9229
lcode launch --name "Python Demo" --mode run
lcode config save --name "Python Profile" --runtime python --entry app.py --cwd . --mode debug
lcode config list
lcode config show --name "Python Profile"
lcode config validate --name "Python Profile"
lcode config validate --all
lcode config run --name "Python Profile"
lcode config run --name "Python Profile" --arg "--feature" --env API_URL=http://127.0.0.1:9000
lcode config run --name "Python Profile" --clear-args --clear-env --env-file ./run.env
lcode config run --name "Python Profile" --env-file ./.env.base --env-file ./.env.local --env API_URL=http://127.0.0.1:9000
lcode config export --file ./.launch-code/profiles.json
lcode config import --file ./.launch-code/profiles.json
lcode config delete --name "Python Profile"
lcode link add --name demo --path /path/to/workspace
lcode link list
lcode list
lcode running
lcode running --wide
lcode running --format wide
lcode running --format default
lcode running --format short
lcode running --format id
lcode running --short-id-len 8
lcode running -q
lcode running --no-headers
lcode list --compact
lcode list --format compact
lcode list --format short
lcode list --format id
lcode list --short-id-len 16
lcode list --compact --no-trunc
lcode list --compact --no-headers
lcode list -q
lcode --link demo list
lcode --local list
lcode link prune --dry-run
lcode link prune
lcode project show
lcode project list
lcode project list --field name --field repository --all
lcode --link demo project show
lcode project set --name "launch-code" --description "IDE-like launch manager" --repository "https://example.com/org/launch-code" --language rust --language python --runtime python --tool debugpy --tag cli
lcode project unset --field tags --field tools
lcode project clear
lcode attach --id <session_id>
lcode dap request --id <session_id> --command initialize --arguments '{"clientID":"launch-code"}'
lcode dap batch --id <session_id> --file ./dap-batch.json --timeout-ms 3000
lcode dap breakpoints --id <session_id> --path ./app.py --line 12 --line 34 --condition "x > 10" --hit-condition "==2" --log-message "value={x}"
lcode dap exception-breakpoints --id <session_id> --filter raised --filter uncaught
lcode dap evaluate --id <session_id> --expression "counter + 1" --frame-id 301 --context watch
lcode dap set-variable --id <session_id> --variables-reference 7001 --name counter --value 42
lcode dap continue --id <session_id> --thread-id 1
lcode dap pause --id <session_id> --thread-id 1
lcode dap next --id <session_id> --thread-id 1
lcode dap step-in --id <session_id> --thread-id 1
lcode dap step-out --id <session_id> --thread-id 1
lcode dap disconnect --id <session_id> --terminate-debuggee
lcode dap terminate --id <session_id> --restart
lcode dap adopt-subprocess --id <session_id> --timeout-ms 2000 --max-events 50
lcode dap threads --id <session_id>
lcode dap stack-trace --id <session_id> --thread-id 1 --levels 20
lcode dap scopes --id <session_id> --frame-id 301
lcode dap variables --id <session_id> --variables-reference 7001 --filter named --start 0 --count 20
lcode dap events --id <session_id> --max 50 --timeout-ms 1000
lcode doctor debug --id <session_id> --tail 80 --max-events 50 --timeout-ms 1500
lcode inspect --id <session_id> --tail 50
lcode inspect <session_id> --tail 50
lcode logs --id <session_id> --tail 200 --follow
lcode logs <session_id> --tail 200 --follow
lcode logs --id <session_id> --tail 500 --contains "ERROR" --contains "Traceback"
lcode logs --id <session_id> --tail 500 --exclude "heartbeat"
lcode logs --id <session_id> --follow --contains "timeout" --ignore-case
lcode logs --id <session_id> --tail 500 --regex "^ERROR\\s+E(100|200)$"
lcode logs --id <session_id> --tail 500 --exclude-regex "^(DEBUG|TRACE)"
lcode serve --bind 127.0.0.1:8787 --token <token>
lcode status --id <session_id>
lcode status <session_id>
lcode list
lcode ps
lcode cleanup
lcode cleanup --dry-run --status stopped
lcode --local cleanup
lcode stop --all --status running --yes
lcode restart --all --dry-run --status running
lcode suspend --all --dry-run --status running
lcode resume --all --dry-run --status suspended
lcode suspend --all --status running --max-failures 1
lcode suspend --all --status running --continue-on-error false
lcode suspend --id <session_id>
lcode suspend <session_id>
lcode resume --id <session_id>
lcode resume <session_id>
lcode restart --id <session_id>
lcode restart <session_id>
lcode stop --id <session_id> --grace-timeout-ms 1500
lcode stop <session_id>
lcode stop --id <session_id> --grace-timeout-ms 100 --force
lcode attach <session_id>
lcode daemon --once
```

Debug output includes endpoint metadata:

- `debug_host`
- `debug_port`
- `requested_debug_port`
- `debug_fallback`
- `debug_endpoint`

`logs` filtering rules:

- `--contains` can be repeated; a line is kept if it matches any token.
- `--exclude` can be repeated; a line is removed if it matches any token.
- `--regex` applies an additional include condition using a regular expression.
- `--exclude-regex` applies a regular expression exclude condition.
- `--ignore-case` applies case-insensitive matching for `--contains`, `--exclude`, `--regex`, and `--exclude-regex`.
- Filtering applies to both `--tail` output and `--follow` stream output.

Session-id commands (`status`, `inspect`, `logs`, `attach`, `stop`, `restart`, `suspend`, `resume`) support both forms:

- `--id <session_id>`
- positional shorthand `<session_id>`

`start` / `debug` / `config run` env override order:

- Saved profile env values are loaded first.
- `--env-file` values are applied in declaration order (`--env-file a --env-file b`, so `b` overrides `a`).
- `--env KEY=VALUE` values are applied last and override both saved env and env-file values.

`start` / `debug` startup log behavior:

- Default mode is background + file log (`--log-mode file`).
- `--tail` keeps background mode but immediately follows the session log until process exit.
- `--foreground --log-mode stdout` streams process output to terminal only.
- `--foreground --log-mode tee` streams to terminal and writes the same output to session log file.
- `--foreground --log-mode file` runs in foreground while writing output to file only.
- `--log-mode stdout|tee` requires `--foreground`.
- `--tail` cannot be combined with `--foreground`.

### Structured CLI Output

Pass `--json` on any command to get machine-readable results.

Success responses:

- Message style: `{"ok":true,"message":"..."}`
- Session command style (`status`/`stop`/`restart`/`suspend`/`resume`): `{"ok":true,"action":"status","message":"...","session":{...}}`
- Batch command style (`stop/restart/suspend/resume --all`): `{"ok":true,"action":"stop","scope":"global","all":true,"matched_count":2,"processed_count":2,"success_count":2,"session_failed_count":0,"link_error_count":0,"failed_count":0,"link_errors":[],"items":[...]}`
- List style: `{"ok":true,"items":[...]}`
- Text block style: `{"ok":true,"text":"..."}`

Error responses are written to stderr:

- `{"ok":false,"error":"<stable_error_code>","message":"<human_readable_text>"}`

Representative error codes:

- `session_not_found`
- `session_missing_pid`
- `session_missing_debug_meta`
- `session_missing_log_path`
- `session_state_changed`
- `profile_not_found`
- `profile_bundle_version_unsupported`
- `profile_validation_failed`
- `invalid_env_pair`
- `invalid_env_file_line`
- `invalid_log_regex`
- `invalid_start_options`
- `confirmation_required`
- `python_debugpy_unavailable`
- `unsupported_debug_runtime`
- `unsupported_dap_runtime`
- `dap_error`
- `http_error`

Session lifecycle operations (`stop` / `restart`) include bounded internal retries for transient
state races in concurrent CLI/HTTP workflows. If all retries still observe conflicting state, APIs
return `session_state_changed` with HTTP 409.

### Doctor Debug Diagnostics

Run:

```bash
lcode doctor debug --id <session_id> --tail 80 --max-events 50 --timeout-ms 1500 --json
```

The response contains:

- `session` and `inspect` snapshots
- `debug.threads` and `debug.events` probe results
- `diagnostics[]` entries with `code`, `level`, `summary`, `detail`, and `suggested_actions`

Diagnostic codes:

- `D001`: Failed to query debug threads.
- `D002`: Failed to read debug events.
- `D003`: Session is not running during failed debug checks.
- `D004`: Debugger warning signature detected in recent log tail.

## HTTP Control Plane

Run the HTTP server:

```bash
lcode serve --bind 127.0.0.1:8787 --token testtoken
```

All endpoints require:

- `Authorization: Bearer <token>`

Core endpoints:

- `GET /v1/health`
- `GET /v1/sessions`
- `GET /v1/sessions/{id}`
- `GET /v1/sessions/{id}/inspect?tail=50`
- `GET /v1/sessions/{id}/debug`
- `POST /v1/sessions/cleanup`
- `GET /v1/project`
- `PUT /v1/project`
- `PATCH /v1/project`
- `DELETE /v1/project`
- `POST /v1/sessions/{id}/stop`
- `POST /v1/sessions/{id}/restart`
- `POST /v1/sessions/{id}/suspend`
- `POST /v1/sessions/{id}/resume`

Debug adapter proxy (DAP over HTTP):

- `POST /v1/sessions/{id}/debug/dap/request` with body `{"command":"...","arguments":{...}}`
- `POST /v1/sessions/{id}/debug/dap/request` with body `{"batch":[{"command":"...","arguments":{}}, ...], "timeout_ms": 5000}`
- `GET /v1/sessions/{id}/debug/dap/events?timeout_ms=1000&max=50`

High-level debug helpers:

- `GET /v1/sessions/{id}/debug/threads`
- `POST /v1/sessions/{id}/debug/breakpoints` with body `{"path":"app.py","lines":[12,34]}`
- `POST /v1/sessions/{id}/debug/breakpoints` with body `{"path":"app.py","lines":[{"line":12,"condition":"x > 10","hitCondition":"==2","logMessage":"value={x}"}]}`
- `POST /v1/sessions/{id}/debug/exception-breakpoints` with body `{"filters":["raised","uncaught"]}`
- `POST /v1/sessions/{id}/debug/evaluate` with body `{"expression":"counter + 1","frameId":301,"context":"watch"}`
- `POST /v1/sessions/{id}/debug/set-variable` with body `{"variablesReference":7001,"name":"counter","value":"42"}`
- `POST /v1/sessions/{id}/debug/continue` with body `{}` or `{"threadId": 1}`
- `POST /v1/sessions/{id}/debug/pause` with body `{}` or `{"threadId": 1}`
- `POST /v1/sessions/{id}/debug/next` with body `{}` or `{"threadId": 1}`
- `POST /v1/sessions/{id}/debug/step-in` with body `{}` or `{"threadId": 1}`
- `POST /v1/sessions/{id}/debug/step-out` with body `{}` or `{"threadId": 1}`
- `POST /v1/sessions/{id}/debug/disconnect` with body `{"terminateDebuggee": true, "suspendDebuggee": false}`
- `POST /v1/sessions/{id}/debug/terminate` with body `{"restart": false}`
- `POST /v1/sessions/{id}/debug/adopt-subprocess` with body `{"timeout_ms":2000,"max_events":50}`

Example:

```bash
curl -sS \
  -H "Authorization: Bearer testtoken" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{"filters":["raised","uncaught"]}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/exception-breakpoints

curl -sS \
  -H "Authorization: Bearer testtoken" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{"expression":"counter + 1","frameId":301,"context":"watch"}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/evaluate

curl -sS \
  -H "Authorization: Bearer testtoken" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{"variablesReference":7001,"name":"counter","value":"42"}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/set-variable

curl -sS \
  -H "Authorization: Bearer testtoken" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{"path":"app.py","lines":[{"line":12,"condition":"x > 10","hitCondition":"==2","logMessage":"value={x}"}]}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/breakpoints

curl -sS \
  -H "Authorization: Bearer testtoken" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{"threadId":1}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/pause

curl -sS \
  -H "Authorization: Bearer testtoken" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{"dry_run":true,"statuses":["stopped"]}' \
  http://127.0.0.1:8787/v1/sessions/cleanup

curl -sS \
  -H "Authorization: Bearer testtoken" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{"threadId":1}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/next

curl -sS \
  -H "Authorization: Bearer testtoken" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{"threadId":1}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/step-in

curl -sS \
  -H "Authorization: Bearer testtoken" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{"threadId":1}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/step-out

curl -sS \
  -H "Authorization: Bearer testtoken" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{"terminateDebuggee":true,"suspendDebuggee":false}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/disconnect

curl -sS \
  -H "Authorization: Bearer testtoken" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{"restart":false}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/terminate

curl -sS \
  -H "Authorization: Bearer testtoken" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{"timeout_ms":2000,"max_events":50}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/adopt-subprocess
```

## Direct DAP CLI

You can issue DAP commands directly without the HTTP control plane:
Direct DAP commands currently support Python debug sessions only.

- `lcode dap request --id <session_id> --command <dap_command> --arguments '{"key":"value"}' --timeout-ms 1500`
- `lcode dap batch --id <session_id> --file ./dap-batch.json --timeout-ms 1500`
- `lcode dap breakpoints --id <session_id> --path ./app.py --line 12 --line 34 [--condition "x > 10"] [--hit-condition "==2"] [--log-message "value={x}"] --timeout-ms 1500`
- `lcode dap exception-breakpoints --id <session_id> [--filter raised --filter uncaught] --timeout-ms 1500`
- `lcode dap evaluate --id <session_id> --expression "counter + 1" [--frame-id 301] [--context watch|repl|hover] --timeout-ms 1500`
- `lcode dap set-variable --id <session_id> --variables-reference 7001 --name counter --value 42 --timeout-ms 1500`
- `lcode dap continue --id <session_id> [--thread-id 1] --timeout-ms 1500`
- `lcode dap pause --id <session_id> [--thread-id 1] --timeout-ms 1500`
- `lcode dap next --id <session_id> [--thread-id 1] --timeout-ms 1500`
- `lcode dap step-in --id <session_id> [--thread-id 1] --timeout-ms 1500`
- `lcode dap step-out --id <session_id> [--thread-id 1] --timeout-ms 1500`
- `lcode dap disconnect --id <session_id> [--terminate-debuggee] [--suspend-debuggee] --timeout-ms 1500`
- `lcode dap terminate --id <session_id> [--restart] --timeout-ms 1500`
- `lcode dap adopt-subprocess --id <session_id> [--timeout-ms 1500] [--max-events 50] [--bootstrap-timeout-ms 5000] [--child-session-id child-id]`
- `lcode dap threads --id <session_id> --timeout-ms 1500`
- `lcode dap stack-trace --id <session_id> [--thread-id 1] [--start-frame 0] [--levels 20] --timeout-ms 1500`
- `lcode dap scopes --id <session_id> --frame-id 301 --timeout-ms 1500`
- `lcode dap variables --id <session_id> --variables-reference 7001 [--filter named|indexed] [--start 0] [--count 20] --timeout-ms 1500`
- `lcode dap events --id <session_id> --max 50 --timeout-ms 1000`

If `--thread-id` is omitted in `dap continue`, the first thread from `threads` is used automatically.
If `--thread-id` is omitted in `dap pause`, the first thread from `threads` is used automatically.
If `--thread-id` is omitted in `dap next`, the first thread from `threads` is used automatically.
If `--thread-id` is omitted in `dap step-in`, the first thread from `threads` is used automatically.
If `--thread-id` is omitted in `dap step-out`, the first thread from `threads` is used automatically.
If `--thread-id` is omitted in `dap stack-trace`, the first thread from `threads` is used automatically.

For Python multiprocessing:

1. Start parent debug session with subprocess hooks enabled (`--subprocess true`, default).
2. Poll events and wait for a `debugpyAttach` event from the parent session.
3. Run `dap adopt-subprocess` (or the HTTP `debug/adopt-subprocess` endpoint) to create a child session id and bootstrap initialize/attach/configurationDone automatically.
4. Send normal `dap` or HTTP debug commands to the returned child session id.

Batch file format (`dap-batch.json`):

```json
[
  {"command": "initialize", "arguments": {"clientID": "launch-code"}},
  {"command": "attach", "arguments": {"justMyCode": false}},
  {"command": "configurationDone", "arguments": {}}
]
```

## Managed Mode

Start a session with `--managed` to auto-restart a dead worker on reconciliation.

- `status` and `list` perform reconciliation for managed sessions
- `daemon --once` performs a single reconciliation pass
- `daemon` runs reconciliation continuously with the configured interval

Example:

```bash
lcode start --runtime python --entry app.py --cwd . --managed
lcode status --id <session_id>
```

## Launch Configuration Support

`lcode launch` can read `.vscode/launch.json` by default.

Supported configuration fields:

- `name`
- `type` (`python`, `node`, `pwa-node`, `node-terminal`, `rust`, `lldb`, `codelldb`)
- `request`
- `program`
- `args`
- `cwd`
- `env`
- `envFile`
- `python`
- `pythonPath`
- `managed`
- `debugHost`
- `debugPort`
- `waitForClient`
- `preLaunchTask`
- `postDebugTask`
- `postStopTask`

Supported launch variables:

- `${workspaceFolder}`
- `${workspaceFolderBasename}`
- `${env:VAR_NAME}`

Example `launch.json`:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "name": "Python Demo",
      "type": "python",
      "request": "launch",
      "program": "app.py",
      "cwd": "${workspaceFolder}",
      "args": ["--env", "dev"]
    }
  ]
}
```

## Saved Profile Management

Use `lcode config` to manage reusable run/debug profiles without editing `launch.json`.

Examples:

```bash
lcode config save --name "Python Run" --runtime python --entry app.py --cwd . --mode run
lcode config save --name "Python Debug" --runtime python --entry app.py --cwd . --mode debug --port 5678
lcode config list
lcode config show --name "Python Debug"
lcode config validate --name "Python Debug"
lcode config validate --all
lcode config run --name "Python Debug"
lcode config run --name "Python Run" --mode debug
lcode config run --name "Python Run" --managed
lcode config run --name "Python Run" --arg "--feature" --env API_URL=http://127.0.0.1:9000
lcode config run --name "Python Run" --clear-args --clear-env --env-file ./run.env
lcode config export --file ./profiles.json
lcode config import --file ./profiles.json
lcode config import --file ./profiles.json --replace
lcode config delete --name "Python Run"
```

Export/import bundle format:

```json
{
  "version": 1,
  "profiles": {
    "Python Run": {
      "name": "Python Run",
      "runtime": "python",
      "entry": "app.py",
      "args": [],
      "cwd": ".",
      "env": {},
      "managed": false,
      "mode": "run",
      "debug": null,
      "prelaunch_task": null,
      "poststop_task": null
    }
  }
}
```

## State Layout

- `.launch-code/state.json`: session records, saved profiles, and runtime metadata
- `.launch-code/logs/<session_id>.log`: process stdout/stderr logs

## Platform Notes

- Unix: full lifecycle support (`stop`, `suspend`, `resume`) via signals
- Windows: process lifecycle commands are supported for start/stop/status; task hooks run through `cmd /C`

## Test and Lint

```bash
cargo test --all -- --nocapture
cargo clippy --all-targets --all-features -- -D warnings
```
