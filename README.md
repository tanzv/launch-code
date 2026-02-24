# launch-code

`launch-code` is a Rust CLI for project run/debug lifecycle management.
Recommended command: `lcode`.
Compatibility command: `launch-code`.

## Key Documentation Links

English:

- [Installation Guide](docs/installation.md)
- [Python Debug Manual](docs/python-debug-manual.md)

Chinese:

- [Chinese README](docs/zh-cn/README.md)
- [Chinese Docs Index](docs/zh-cn/index.md)

## Why launch-code

- Global project visibility: view and manage sessions across linked workspaces from any directory.
- Daily lifecycle operations: start, stop, restart, suspend, resume, and log inspection for active development loops.
- Multi-runtime project workflows: run and debug Python/Node projects and run Rust projects in one CLI.
- Debug and diagnostics workflows: attach, DAP commands, runtime checks, and debug health checks for troubleshooting.
- Automation and platform integration: stable `--json` output and machine-readable error codes for scripts and CI tooling.

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

Enter the linked workspace:

```bash
cd /path/to/workspace
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

Single-target lifecycle:

```bash
lcode start ...
lcode debug ...
lcode stop --id <id>
lcode stop <id>
lcode restart --id <id>
lcode suspend --id <id>
lcode resume --id <id>
```

Batch lifecycle (global-aware):

```bash
lcode stop --all --dry-run
lcode stop --all --yes
lcode restart --all --dry-run
lcode suspend --all --dry-run
lcode resume --all --dry-run
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

- [Installation Guide (EN)](docs/installation.md)
- [Python Debug Manual (EN)](docs/python-debug-manual.md)
- [Chinese README](docs/zh-cn/README.md)
- [Chinese Docs Index](docs/zh-cn/index.md)

## Development Verification

Run before merging:

```bash
cargo test -q
cargo clippy --all-targets --all-features -- -D warnings
```
