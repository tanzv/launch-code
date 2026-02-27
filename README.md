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
- Multi-runtime project workflows: run and debug Python/Node/Go projects and run Rust projects in one CLI.
- Cross-platform build workflow: build Rust/Go artifacts for common Linux/macOS/Windows targets with one command.
- Debug and diagnostics workflows: attach, DAP commands (including repeated Go DAP calls), runtime checks, and debug health checks for troubleshooting.
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

## BPMN Package Release

The BPMN package is published from `packages/npm/bpmn`.
Use the orchestrator script to run the complete workflow (prepare, preflight, publish):

```bash
./scripts/release_bpmn_package.sh --package-dir ./packages/npm/bpmn
```

Shortcut command:

```bash
./scripts/publish_bpmn.sh
```

Configure publish registry source via local environment variable:

```bash
export LCODE_NPM_PUBLISH_REGISTRY="https://registry.npmjs.org"
./scripts/release_bpmn_package.sh --package-dir ./packages/npm/bpmn
```

Use a local env file configuration:

```bash
cp ./packages/npm/bpmn/.env.release.example ./packages/npm/bpmn/.env.release.local
./scripts/publish_bpmn.sh --dry-run
```

Dry-run example:

```bash
./scripts/release_bpmn_package.sh --package-dir ./packages/npm/bpmn --dry-run
```

Release artifacts and process docs:

- [BPMN Release Workflow](docs/release/npm-publish.md)
- [BPMN Workflow Definition](docs/release/npm-publish.bpmn)

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

Start a Go debug session (Delve headless multi-client debug):

```bash
lcode debug --runtime go --entry ./cmd/app --cwd . --host 127.0.0.1 --port 43000
```

Build a Rust binary for Linux amd64:

```bash
lcode build --runtime rust --cwd . --entry lcode --platform linux/amd64 --release
```

Build a Go binary for macOS arm64:

```bash
lcode build --runtime go --cwd . --entry ./cmd/service --platform darwin/arm64 --output ./dist/service
```

Debug Go tests with Delve:

```bash
lcode debug --runtime go --go-mode test --entry ./pkg/service --cwd . --arg=-test.run --arg=TestServiceFlow
```

Attach to an existing Go process:

```bash
lcode debug --runtime go --go-mode attach --entry 12345 --cwd . --host 127.0.0.1 --port 43000
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
- Interactive `lcode list` / `lcode ps` defaults to compact columns on TTY; non-interactive output keeps wide columns for script readability.
- On TTY, `lcode ps` defaults to interactive browser mode (navigation keys and detail panel). Use `--no-interactive` to force plain table output.
- Use `--link <name>` to scope to one linked workspace.
- Use `--local` to force current workspace scope.
- When `LAUNCH_CODE_HOME` is set and `--global` is not provided, runtime write commands stay local and do not require writing global link metadata.
- Session-id lifecycle commands support cross-link fallback by id in global default mode, including multi-id positional usage (`lcode stop <id1> <id2>`, and same pattern for `restart`/`suspend`/`resume`).

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
lcode build --runtime rust --platform linux/amd64 --release --entry lcode
lcode build --runtime go --platform windows/arm64 --entry ./cmd/service --output ./dist/service.exe
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
lcode list --sort updated --limit 20
lcode running
lcode running --sort name --limit 10
lcode status --id <id>
lcode inspect --id <id>
lcode logs --id <id> --follow
lcode logs --id <id> --tail 200 --since 10m --until 1m --timestamps
```

Debug and diagnostics:

```bash
lcode attach --id <id>
lcode dap ...
lcode doctor runtime
lcode doctor debug --id <id>
lcode doctor all --runtime node --strict --json
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
- Interactive browser: `lcode ps` (default on TTY) or `lcode list --interactive` (`q` quit, `j/k` or arrows move, `Enter` toggle details, `r` refresh).
- Empty list/running output prints next-step hints (`lcode running`, `lcode list --format id`, `lcode link list`).
- Session list ordering/paging: `--sort id|name|runtime|status|updated|restarts --limit <N>`
- Watch mode: `--watch [INTERVAL] --watch-count <N>`
- Log time window and timestamp prefix: `logs --since <TIME> --until <TIME> --timestamps`
- Cross-build dry run (no execution): `build --dry-run`
- Timing diagnostics: `--trace-time`
- Global cleanup JSON includes `link_errors` and `link_error_count` for unreadable/broken links.

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
