# Installation Guide

This guide provides complete install, upgrade, verification, and troubleshooting steps for `launch-code`.

## Commands Installed

- `lcode` (recommended)
- `launch-code` (compatibility command)

## Option A: One-Click Install (Recommended)

```bash
bash ./scripts/install.sh
```

What this script does:

1. Ensures Rust/Cargo is available (installs via `rustup` when missing).
2. Installs the CLI from current repository (`cargo install --path . --force`).
3. Performs best-effort debug dependency setup:
   - Python: `debugpy`
   - Node: js-debug adapter discovery/installation
   - Go: `dlv` (installed when Go toolchain is detected)

### Installer Options

Install CLI only:

```bash
bash ./scripts/install.sh --no-debug-deps
```

Require debug dependencies to be ready:

```bash
bash ./scripts/install.sh --strict-debug-deps
```

Show options:

```bash
bash ./scripts/install.sh --help
```

## Option B: Manual Install

Build:

```bash
cargo build
```

Install:

```bash
cargo install --path . --force
```

## Upgrade

From repository root:

```bash
git pull
bash ./scripts/install.sh
```

Or:

```bash
cargo install --path . --force
```

## Verify Installation

```bash
lcode --version
launch-code --version
lcode --help
lcode doctor runtime --json
```

## PATH Setup

If `lcode` is not found after installation, add Cargo bin directory:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

Persist this line in your shell profile (for example `~/.zshrc`).

## Debug Dependency Notes

Python:

```bash
python3 -m pip install --user debugpy
```

Go (Delve):

```bash
go install github.com/go-delve/delve/cmd/dlv@latest
```

Node adapter resolution order:

1. `LCODE_NODE_DAP_ADAPTER_CMD` (JSON array command, highest priority)
2. `js-debug-adapter` in `PATH`
3. VSCode/Cursor extension script auto-discovery (`dapDebugServer.js`)

Example explicit adapter command:

```bash
export LCODE_NODE_DAP_ADAPTER_CMD='["node","/path/to/js-debug/src/dapDebugServer.js"]'
```

## Common Troubleshooting

### 1) `lcode: command not found`

- Confirm installation succeeded.
- Add `$HOME/.cargo/bin` to `PATH`.
- Open a new shell and re-run `lcode --version`.

### 2) Node adapter install fails (`npm 404` or mirror issue)

- Installation can still complete successfully for CLI usage.
- Configure adapter explicitly using `LCODE_NODE_DAP_ADAPTER_CMD`.
- Run `lcode doctor runtime --runtime node --json` to check readiness.

### 3) Python debug is not ready

- Install `debugpy` for the same interpreter used by sessions.
- Confirm with:

```bash
python3 -c "import debugpy; print(debugpy.__version__)"
```

### 4) Strict install fails

If you use `--strict-debug-deps`, installer exits non-zero whenever debug dependencies are incomplete.
Use default mode or `--no-debug-deps` when you only need lifecycle commands.
When Go is available in PATH, strict mode also validates `dlv` readiness.

### 5) Go debug is not ready

- Install Delve (`dlv`) and ensure it is in PATH.
- Confirm with:

```bash
dlv version
lcode doctor runtime --runtime go --json
```

## Post-Install Health Check

```bash
lcode doctor runtime
lcode doctor runtime --strict --runtime python --json
```

For command routing and global visibility:

```bash
lcode link list
lcode list
lcode running
```
