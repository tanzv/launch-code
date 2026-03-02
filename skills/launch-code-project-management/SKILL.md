---
name: launch-code-project-management
description: Use when operating lcode sessions for run/debug lifecycle management, cross-workspace routing, and diagnostics in local development or CI.
---

# Launch-Code Project Management

## Overview

Use this skill for day-to-day `lcode` operations: run/debug startup, session supervision, cross-workspace routing, diagnostics, and cleanup.

## When to Use

- Need to start or debug a project with reproducible CLI commands.
- Need to inspect, control, or clean up sessions across local/global/link scopes.
- Need machine-readable output for scripts or CI (`--json`).
- Need runtime or debug diagnostics (`doctor`, `dap`, `inspect`, `logs`).

Do not use this skill for non-operational planning topics.

## Hard Constraints

- Use `lcode` as the primary command (`launch-code` is compatibility only).
- Scope is global by default unless `--local` or `--link <name>` is explicitly set.
- For global batch lifecycle changes, run `--dry-run` first; non-dry-run apply must include `--yes`.
- Prefer `--json` for automation and contract validation.
- Prefer `lcode launch --name ...` with `launch.json` as the canonical startup source for team workflows.
- `lcode launch` config resolution is deterministic: `--launch-file` override first, then `.vscode/launch.json`, then `.launch-code/launch.json`.
- `lcode config` profiles are persisted in `<workspace>/.launch-code/state.json` (`profiles`) and are intended for local temporary overrides.
- Session-id commands should support both forms when available:
  - `--id <session_id>`
  - `<session_id>` positional shorthand
- `lcode ps` is an alias of `lcode list`.
- Interactive browser mode is explicit: `lcode ps --interactive` or `lcode list --interactive`.

## Quick Workflow

1. Install/upgrade and validate runtime readiness.

```bash
cargo install --path . --force
lcode doctor runtime --strict --json
```

2. Verify routing and discover active sessions.

```bash
lcode link list --json
lcode list
lcode running
```

3. Start or debug workload.

```bash
lcode start --runtime python --entry app.py --cwd .
lcode debug --runtime python --entry app.py --cwd . --host 127.0.0.1 --port 5678
```

4. Inspect and troubleshoot.

```bash
lcode status <session_id> --json
lcode inspect <session_id> --tail 100
lcode logs <session_id> --tail 200 --since 10m --timestamps
lcode doctor debug --id <session_id> --json
```

5. Apply lifecycle changes safely.

```bash
lcode stop --all --status running --dry-run --json
lcode stop --all --status running --yes --summary
```

## Extended References

- Full operational reference: `skills/launch-code-project-management/references/operational-reference.md`
- Chinese command reference: `docs/zh-cn/command-reference.md`

## Verification Commands

```bash
cargo test -q
cargo clippy --all-targets --all-features -- -D warnings
```
