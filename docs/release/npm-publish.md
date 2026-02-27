# BPMN Package Publish Workflow

## Current Release Form

`launch-code` now publishes BPMN artifacts as a dedicated npm package:

- Package path: `packages/npm/bpmn`
- Package name: `launch-code`
- Primary artifact: `bpmn/npm-publish.bpmn`

## BPMN Source of Truth

- Workflow file: `docs/release/npm-publish.bpmn`
- Workflow documentation: `docs/release/npm-publish.md`

## Complete Release Process

Use the orchestrator script as the default release entrypoint:

```bash
./scripts/release_bpmn_package.sh --package-dir ./packages/npm/bpmn
```

Shortcut command:

```bash
./scripts/publish_bpmn.sh
```

The orchestrator performs:

1. Optional workspace cleanliness check (git).
2. Optional repository verification (`cargo test -q` and `cargo clippy --all-targets --all-features -- -D warnings`).
3. Package preparation (`scripts/prepare_bpmn_package.sh`) from docs source.
4. Optional package version update (`npm version ... --no-git-tag-version`).
5. Preflight pack check (`npm pack --dry-run`).
6. Publish execution (`scripts/npm_publish.sh`).

## Recommended Commands

Publish with explicit version and tag:

```bash
./scripts/release_bpmn_package.sh \
  --package-dir ./packages/npm/bpmn \
  --version 0.2.0 \
  --tag latest
```

Dry-run end-to-end:

```bash
./scripts/release_bpmn_package.sh \
  --package-dir ./packages/npm/bpmn \
  --dry-run
```

Use a local registry source override:

```bash
export LCODE_NPM_PUBLISH_REGISTRY="https://registry.npmjs.org"
./scripts/release_bpmn_package.sh --package-dir ./packages/npm/bpmn
```

Use local env file defaults:

```bash
cp ./packages/npm/bpmn/.env.release.example ./packages/npm/bpmn/.env.release.local
./scripts/publish_bpmn.sh --dry-run
```

Explicit env file path:

```bash
./scripts/release_bpmn_package.sh \
  --package-dir ./packages/npm/bpmn \
  --env-file ./packages/npm/bpmn/.env.release.local
```

## Environment Variables

- `LCODE_NPM_PUBLISH_REGISTRY`: publish registry source (higher priority than `NPM_CONFIG_REGISTRY`).
- `NPM_CONFIG_REGISTRY`: fallback publish registry source.
- `LCODE_BPMN_PACKAGE_DIR`: default BPMN package directory.
- `LCODE_BPMN_SOURCE_FILE`: default BPMN source file path.
- `LCODE_BPMN_RELEASE_ENV_FILE`: default env file path.
- `LCODE_NPM_BPMN_VERSION`: default version used by release orchestrator.
- `LCODE_NPM_PUBLISH_TAG`: default publish tag used by `scripts/npm_publish.sh`.
- `LCODE_NPM_PUBLISH_ACCESS`: default publish access mode.
- `LCODE_NPM_PUBLISH_DRY_RUN`: enables dry-run in publish script when truthy.

## Registry Resolution Order

1. `--registry` argument passed to publish scripts
2. `LCODE_NPM_PUBLISH_REGISTRY`
3. `NPM_CONFIG_REGISTRY`
4. npm default resolution
