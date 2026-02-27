# launch-code

BPMN workflow artifacts used by launch-code release automation.

## Contents

- `bpmn/npm-publish.bpmn`: release process BPMN definition.
- `docs/npm-publish.md`: release process documentation.

## Publish

Use repository release script from project root:

```bash
./scripts/release_bpmn_package.sh --package-dir ./packages/npm/bpmn
```

Shortcut command from project root:

```bash
./scripts/publish_bpmn.sh
```

Local env file setup:

```bash
cp ./packages/npm/bpmn/.env.release.example ./packages/npm/bpmn/.env.release.local
./scripts/publish_bpmn.sh --dry-run
```
