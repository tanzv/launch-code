# Session Operations Checklist

This checklist is optimized for daily `lcode` usage in project startup, maintenance, and debugging.

## 1. Discover scope and active sessions

```bash
lcode link list
lcode list
lcode running
lcode ps
```

## 2. Launch and debug

```bash
lcode start --runtime python --entry app.py --cwd .
lcode debug --runtime python --entry app.py --cwd . --host 127.0.0.1 --port 5678
lcode debug --runtime go --entry ./cmd/app --cwd . --host 127.0.0.1 --port 43000
lcode debug --runtime go --go-mode test --entry ./pkg/service --cwd . --arg=-test.run --arg=TestServiceFlow
lcode debug --runtime go --go-mode attach --entry 12345 --cwd . --host 127.0.0.1 --port 43000
lcode status <session_id>
lcode inspect <session_id> --tail 100
lcode logs <session_id> --tail 200 --follow
lcode attach <session_id>
```

## 3. Lifecycle maintenance

```bash
lcode stop <session_id>
lcode restart <session_id>
lcode suspend <session_id>
lcode resume <session_id>
```

Batch operations:

```bash
lcode stop --all --status running --yes
lcode restart --all --dry-run --status running
lcode suspend --all --dry-run --status running
lcode resume --all --dry-run --status suspended
```

## 4. Global and link routing checks

```bash
lcode project show
lcode --link <name> list
lcode --local list
```

## 5. Debug diagnostics

```bash
lcode doctor debug --id <session_id> --tail 80 --max-events 50 --timeout-ms 1500
lcode dap threads --id <session_id>
lcode dap stack-trace --id <session_id> --thread-id 1 --levels 20
```

## 6. State hygiene

```bash
lcode cleanup --dry-run --status stopped
lcode cleanup
lcode link prune --dry-run
lcode link prune
```

## 7. Verification after changes

```bash
cargo test -q
cargo clippy --all-targets --all-features -- -D warnings
```
