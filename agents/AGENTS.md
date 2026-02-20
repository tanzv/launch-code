# Agents Workspace

This directory stores agent-facing operational artifacts for the `launch-code` repository.

## Inheritance

All rules in repository-root `AGENTS.md` apply here by default.

## Intended usage

Keep this directory focused on practical maintenance and debugging support:

- Command quick references for run/debug/session maintenance.
- Reusable operational checklists.
- Snapshots or generated indexes useful for troubleshooting.
- Small helper scripts for repeatable local workflows.

Primary support scope:

- Project startup and process supervision routines.
- Session lifecycle maintenance and recovery playbooks.
- Debug attach and diagnostics operation checklists.
- Global/link/local routing troubleshooting references.

## Guardrails

- Keep artifacts repository-relative and portable.
- Do not commit secrets, tokens, or private environment values.
- Prefer plain text formats (`.md`, `.tsv`, `.json`) for easy diff review.
- Ensure documented commands match current CLI behavior.

Command contract alignment:

- Keep examples aligned with current `lcode` behavior.
- Prefer Docker-like lifecycle command ergonomics in examples.
- Cover both `--id <session_id>` and positional `<session_id>` forms where applicable.
- Keep global-default behavior explicit in operational notes.

Output and automation alignment:

- Keep table-style examples readable for terminal usage.
- Keep JSON-mode examples stable and machine-consumable.
- Prefer deterministic field names and ordering in documentation snippets.

## Recommended update triggers

Update files in this directory when:

- Session lifecycle command semantics change.
- Debugging workflows are updated.
- Global/link/local routing behavior changes.
- New operational diagnostics are added.
- List/running display format changes.
- Error-code or JSON response schema changes.

## Validation checklist for artifact updates

Before marking updates complete:

- Confirm referenced commands still exist in `lcode --help`.
- Confirm examples are consistent with `README.md` and skill docs.
- Keep command lines copy-paste friendly.
