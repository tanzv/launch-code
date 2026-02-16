# launch-code

`launch-code` is a Rust CLI that provides IDE-like run management for local development workflows.

## Features

- Multi-runtime launch adapters: Python, Node, Rust
- Run and debug modes (`start`, `debug`)
- Workspace session persistence (`.launch-code/state.json`)
- Atomic state updates for concurrent CLI/HTTP writers (multi-process safe persistence)
- Process lifecycle controls (`stop`, `restart`, `suspend`, `resume`)
- Graceful/forced stop strategy (`stop --grace-timeout-ms`, optional `--force`)
- Managed sessions with automatic restart after worker exit
- Reconciliation daemon (`daemon`)
- HTTP control plane (`serve`) for status, lifecycle commands, and debug proxying
- VS Code `launch.json` compatibility (`launch` command)
- Saved profile management (`config save/list/show/run/delete`)
- `preLaunchTask` and `postStopTask` hooks for launch configurations
- Debug port conflict fallback with session metadata output
- Structured CLI output (`--json`) with stable machine-readable error codes

## Install and Build

```bash
cargo build
```

## Documentation

- `docs/python-debug-manual.md`: End-to-end Python debug workflow for CLI and HTTP.
- `docs/examples/python-debug-demo/app.py`: Minimal Python script for breakpoint and stepping demos.

## Python Debug Requirements

Python debug mode uses `debugpy`.

```bash
python -m pip install debugpy
```

Optional interpreter override for run/debug sessions:

```bash
launch-code debug --runtime python --entry app.py --cwd . --env PYTHON_BIN=/path/to/python
launch-code debug --runtime python --entry app.py --cwd . --subprocess true
```

## CLI Commands

```bash
launch-code start --runtime python --entry app.py --cwd .
launch-code debug --runtime python --entry app.py --cwd . --host 127.0.0.1 --port 5678 --subprocess true
launch-code launch --name "Python Demo" --mode run
launch-code config save --name "Python Profile" --runtime python --entry app.py --cwd . --mode debug
launch-code config list
launch-code config show --name "Python Profile"
launch-code config validate --name "Python Profile"
launch-code config validate --all
launch-code config run --name "Python Profile"
launch-code config run --name "Python Profile" --arg "--feature" --env API_URL=http://127.0.0.1:9000
launch-code config run --name "Python Profile" --clear-args --clear-env --env-file ./run.env
launch-code config run --name "Python Profile" --env-file ./.env.base --env-file ./.env.local --env API_URL=http://127.0.0.1:9000
launch-code config export --file ./.launch-code/profiles.json
launch-code config import --file ./.launch-code/profiles.json
launch-code config delete --name "Python Profile"
launch-code attach --id <session_id>
launch-code dap request --id <session_id> --command initialize --arguments '{"clientID":"launch-code"}'
launch-code dap batch --id <session_id> --file ./dap-batch.json --timeout-ms 3000
launch-code dap breakpoints --id <session_id> --path ./app.py --line 12 --line 34 --condition "x > 10" --hit-condition "==2" --log-message "value={x}"
launch-code dap exception-breakpoints --id <session_id> --filter raised --filter uncaught
launch-code dap evaluate --id <session_id> --expression "counter + 1" --frame-id 301 --context watch
launch-code dap set-variable --id <session_id> --variables-reference 7001 --name counter --value 42
launch-code dap continue --id <session_id> --thread-id 1
launch-code dap pause --id <session_id> --thread-id 1
launch-code dap next --id <session_id> --thread-id 1
launch-code dap step-in --id <session_id> --thread-id 1
launch-code dap step-out --id <session_id> --thread-id 1
launch-code dap disconnect --id <session_id> --terminate-debuggee
launch-code dap terminate --id <session_id> --restart
launch-code dap adopt-subprocess --id <session_id> --timeout-ms 2000 --max-events 50
launch-code dap threads --id <session_id>
launch-code dap stack-trace --id <session_id> --thread-id 1 --levels 20
launch-code dap scopes --id <session_id> --frame-id 301
launch-code dap variables --id <session_id> --variables-reference 7001 --filter named --start 0 --count 20
launch-code dap events --id <session_id> --max 50 --timeout-ms 1000
launch-code inspect --id <session_id> --tail 50
launch-code logs --id <session_id> --tail 200 --follow
launch-code logs --id <session_id> --tail 500 --contains "ERROR" --contains "Traceback"
launch-code logs --id <session_id> --tail 500 --exclude "heartbeat"
launch-code logs --id <session_id> --follow --contains "timeout" --ignore-case
launch-code logs --id <session_id> --tail 500 --regex "^ERROR\\s+E(100|200)$"
launch-code logs --id <session_id> --tail 500 --exclude-regex "^(DEBUG|TRACE)"
launch-code serve --bind 127.0.0.1:8787 --token <token>
launch-code status --id <session_id>
launch-code list
launch-code suspend --id <session_id>
launch-code resume --id <session_id>
launch-code restart --id <session_id>
launch-code stop --id <session_id> --grace-timeout-ms 1500
launch-code stop --id <session_id> --grace-timeout-ms 100 --force
launch-code daemon --once
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

`config run` env override order:

- Saved profile env values are loaded first.
- `--env-file` values are applied in declaration order (`--env-file a --env-file b`, so `b` overrides `a`).
- `--env KEY=VALUE` values are applied last and override both saved env and env-file values.

### Structured CLI Output

Pass `--json` on any command to get machine-readable results.

Success responses:

- Message style: `{"ok":true,"message":"..."}`
- List style: `{"ok":true,"items":[...]}`
- Text block style: `{"ok":true,"text":"..."}`

Error responses are written to stderr:

- `{"ok":false,"error":"<stable_error_code>","message":"<human_readable_text>"}`

Representative error codes:

- `session_not_found`
- `session_missing_pid`
- `session_missing_debug_meta`
- `session_missing_log_path`
- `profile_not_found`
- `profile_validation_failed`
- `invalid_env_pair`
- `invalid_env_file_line`
- `invalid_log_regex`
- `python_debugpy_unavailable`
- `dap_error`
- `http_error`

## HTTP Control Plane

Run the HTTP server:

```bash
launch-code serve --bind 127.0.0.1:8787 --token testtoken
```

All endpoints require:

- `Authorization: Bearer <token>`

Core endpoints:

- `GET /v1/health`
- `GET /v1/sessions`
- `GET /v1/sessions/{id}`
- `GET /v1/sessions/{id}/inspect?tail=50`
- `GET /v1/sessions/{id}/debug`
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

- `launch-code dap request --id <session_id> --command <dap_command> --arguments '{"key":"value"}' --timeout-ms 1500`
- `launch-code dap batch --id <session_id> --file ./dap-batch.json --timeout-ms 1500`
- `launch-code dap breakpoints --id <session_id> --path ./app.py --line 12 --line 34 [--condition "x > 10"] [--hit-condition "==2"] [--log-message "value={x}"] --timeout-ms 1500`
- `launch-code dap exception-breakpoints --id <session_id> [--filter raised --filter uncaught] --timeout-ms 1500`
- `launch-code dap evaluate --id <session_id> --expression "counter + 1" [--frame-id 301] [--context watch|repl|hover] --timeout-ms 1500`
- `launch-code dap set-variable --id <session_id> --variables-reference 7001 --name counter --value 42 --timeout-ms 1500`
- `launch-code dap continue --id <session_id> [--thread-id 1] --timeout-ms 1500`
- `launch-code dap pause --id <session_id> [--thread-id 1] --timeout-ms 1500`
- `launch-code dap next --id <session_id> [--thread-id 1] --timeout-ms 1500`
- `launch-code dap step-in --id <session_id> [--thread-id 1] --timeout-ms 1500`
- `launch-code dap step-out --id <session_id> [--thread-id 1] --timeout-ms 1500`
- `launch-code dap disconnect --id <session_id> [--terminate-debuggee] [--suspend-debuggee] --timeout-ms 1500`
- `launch-code dap terminate --id <session_id> [--restart] --timeout-ms 1500`
- `launch-code dap adopt-subprocess --id <session_id> [--timeout-ms 1500] [--max-events 50] [--bootstrap-timeout-ms 5000] [--child-session-id child-id]`
- `launch-code dap threads --id <session_id> --timeout-ms 1500`
- `launch-code dap stack-trace --id <session_id> [--thread-id 1] [--start-frame 0] [--levels 20] --timeout-ms 1500`
- `launch-code dap scopes --id <session_id> --frame-id 301 --timeout-ms 1500`
- `launch-code dap variables --id <session_id> --variables-reference 7001 [--filter named|indexed] [--start 0] [--count 20] --timeout-ms 1500`
- `launch-code dap events --id <session_id> --max 50 --timeout-ms 1000`

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
launch-code start --runtime python --entry app.py --cwd . --managed
launch-code status --id <session_id>
```

## Launch Configuration Support

`launch-code launch` can read `.vscode/launch.json` by default.

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

Use `launch-code config` to manage reusable run/debug profiles without editing `launch.json`.

Examples:

```bash
launch-code config save --name "Python Run" --runtime python --entry app.py --cwd . --mode run
launch-code config save --name "Python Debug" --runtime python --entry app.py --cwd . --mode debug --port 5678
launch-code config list
launch-code config show --name "Python Debug"
launch-code config validate --name "Python Debug"
launch-code config validate --all
launch-code config run --name "Python Debug"
launch-code config run --name "Python Run" --mode debug
launch-code config run --name "Python Run" --managed
launch-code config run --name "Python Run" --arg "--feature" --env API_URL=http://127.0.0.1:9000
launch-code config run --name "Python Run" --clear-args --clear-env --env-file ./run.env
launch-code config export --file ./profiles.json
launch-code config import --file ./profiles.json
launch-code config import --file ./profiles.json --replace
launch-code config delete --name "Python Run"
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
