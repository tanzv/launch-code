# launch-code

`launch-code` is a Rust CLI for project run/debug lifecycle management.
Recommended command: `lcode`.
Compatibility command: `launch-code`.

## Why launch-code

- Global-first visibility: list sessions across linked workspaces by default.
- Docker-like lifecycle ergonomics: `start`, `stop`, `restart`, `running`, `logs`.
- Multi-runtime support: Python, Node, Rust.
- Debug workflows: Python debug is built-in, Node debug is adapter-based, Rust is run-ready.
- Script-friendly automation: stable `--json` output and machine-readable error codes.

## Install

Build from source:

```bash
cargo build
```

Install CLI binaries:

```bash
cargo install --path . --force
```

One-click installer:

```bash
bash ./scripts/install.sh
```

Installer variants:

```bash
bash ./scripts/install.sh --no-debug-deps
bash ./scripts/install.sh --strict-debug-deps
```

Verify installation:

```bash
lcode --version
launch-code --version
lcode doctor runtime --json
```

## Quick Start

Register a workspace link (global metadata):

```bash
lcode link add --name demo --path /path/to/workspace
```

Start a Python session:

```bash
lcode start --runtime python --entry app.py --cwd .
```

Start a Python debug session:

```bash
lcode debug --runtime python --entry app.py --cwd . --host 127.0.0.1 --port 5678
```

Check sessions:

```bash
lcode list
lcode running
```

Stop sessions:

```bash
lcode stop --all --yes
lcode stop <session_id>
```

## Scope Model (Global by Default)

- Global link registry: `$HOME/.launch-code/links.json`
- Workspace runtime state: `<workspace>/.launch-code/state.json`
- `lcode list` and `lcode running` default to global aggregation across links.
- Use `--link <name>` to scope to one linked workspace.
- Use `--local` to force current workspace scope.

Helpful maintenance commands:

```bash
lcode link list
lcode link prune --dry-run
lcode link prune
lcode cleanup
```

## Command Surface

Core lifecycle:

```bash
lcode start ...
lcode debug ...
lcode stop [--id <id>|<id>] [--all]
lcode restart [--id <id>|<id>] [--all]
lcode suspend [--id <id>|<id>] [--all]
lcode resume [--id <id>|<id>] [--all]
```

Discovery and inspection:

```bash
lcode list
lcode running
lcode status --id <id>
lcode inspect --id <id>
lcode logs --id <id> --follow
```

Debug and diagnostics:

```bash
lcode attach --id <id>
lcode dap ...
lcode doctor runtime
lcode doctor debug --id <id>
```

Project and profile management:

```bash
lcode project show
lcode project list
lcode project set ...
lcode config save ...
lcode config list
lcode config run --name <profile>
```

## Output and Performance Options

- JSON mode: `--json`
- Session list formats: `--format table|compact|wide|id`
- Watch mode: `--watch [INTERVAL] --watch-count <N>`
- Timing diagnostics: `--trace-time`

## Documentation

English:

- `docs/installation.md`
- `docs/python-debug-manual.md`

Chinese:

- `docs/zh-cn/README.md`
- `docs/zh-cn/index.md`
- `docs/zh-cn/installation.md`
- `docs/zh-cn/quick-start.md`
- `docs/zh-cn/command-reference.md`
- `docs/zh-cn/json-error-codes.md`
- `docs/zh-cn/http-api.md`
- `docs/zh-cn/runtime-debug-matrix.md`
- `docs/zh-cn/troubleshooting.md`

## Development Verification

Run before merging:

```bash
cargo test -q
cargo clippy --all-targets --all-features -- -D warnings
```
