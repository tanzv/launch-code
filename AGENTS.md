# AGENTS Instructions for launch-code

These instructions apply to the entire repository.

## Language and content rules

- Respond to users in Chinese.
- Keep generated source content in English only.
- Keep all code comments in English.
- Do not add Chinese text in non-documentation source files.
- Chinese is allowed in documentation under `docs/` and `user-docs/`.

## Repository focus

The primary goal of this project is to provide a practical local project runtime manager for:

- Project startup and process supervision.
- Session lifecycle control and maintenance.
- Debug startup, attach, and diagnostics workflows.
- Daily developer assistance for run/debug/inspect/cleanup operations.

Core usage targets:

- Docker-like lifecycle ergonomics for routine session operations.
- Global-first project/session visibility with link-based routing.
- Reliable developer debugging workflows (attach, diagnostics, DAP checks).

## CLI quality rules

- Keep command behavior consistent across local, `--link`, and global modes.
- Ensure session-id commands support clear and predictable routing.
- Keep command help concise, actionable, and copy-paste friendly.
- Prefer Docker-like ergonomics for core lifecycle commands.
- Align CLI examples in `README.md`, skills, and tests whenever behavior changes.

Command behavior contracts:

- Global is default unless `--local` or `--link` is explicitly set.
- `lcode list` and `lcode running` should remain fast and readable in global scope.
- `lcode ps` is an alias of `lcode list`.
- Session-id commands should support both forms whenever implemented:
  - `--id <session_id>`
  - `<session_id>` positional shorthand
- Global session-id fallback should route by id across links when current workspace lookup misses.

Lifecycle command consistency:

- Keep single-session and batch (`--all`) semantics explicit and non-ambiguous.
- Batch commands must support deterministic filtering and failure control.
- Global batch operations must tolerate broken links and report link-level errors.

Output contracts:

- `--json` output schema must remain stable and script-friendly.
- Text output should prioritize scanability in terminal tables.
- Maintain consistent headers and field ordering for `list`/`running` text views.
- Error output must include stable error codes in JSON mode.

## Development workflow

- Make minimal, targeted edits for each request.
- Avoid unrelated refactors unless required to fix correctness or reliability.
- Update tests for every behavior change.
- Prefer deterministic output formats for automation (`--json` paths).
- Keep text output readable for interactive terminal usage.

Performance and usability guardrails:

- Avoid expensive full-scan logic on hot paths when cached routing/index is available.
- Keep link-registry scans resilient; skip unreadable links without aborting global commands.
- Preserve fast-path behavior for empty or small session sets.
- Keep command latency and output clarity balanced for daily interactive use.

Debugging and maintenance expectations:

- Ensure debug workflows are runnable end-to-end (`debug`, `attach`, `inspect`, `logs`, `doctor`, `dap`).
- Keep session cleanup and session index mappings consistent after deletion.
- Validate process-state transitions (`running`, `stopped`, `suspended`, `unknown`) carefully.

## Verification before completion

- Run targeted tests first for touched behavior.
- Run full regression before claiming completion:
  - `cargo test -q`
  - `cargo clippy --all-targets --all-features -- -D warnings`
- If verification cannot run, report exactly what is missing.

Minimum targeted checks for CLI behavior changes:

- `tests/cli_help.rs`
- `tests/cli_json_output.rs`
- `tests/cli_list_filters.rs`
- Additional command-specific suites touched by the change.

## Documentation and skill sync

When command behavior changes, update:

- `README.md`
- `skills/launch-code-project-management/SKILL.md`
- `agents/` operational notes when relevant

Install/update expectations after CLI changes:

- Reinstall local binary for validation:
  - `cargo install --path . --force`
- Verify representative commands in local shell before completion.

## Agent workspace notes

Use the `agents/` directory for agent-facing operational artifacts (quick references,
checklists, snapshots, and scripts that help maintain and debug project sessions).
