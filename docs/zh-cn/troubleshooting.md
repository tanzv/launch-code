# 故障排查手册（中文）

本文档覆盖 `lcode` 常见问题、定位顺序和修复建议。

## 排查总流程

1. 先确认作用域（默认全局）：`--global` / `--local` / `--link`
2. 看会话可见性：`lcode link list`、`lcode list`
3. 看会话状态：`lcode status --id <session_id>`、`lcode inspect --id <session_id>`
4. 看环境健康：`lcode doctor runtime`
5. 看调试健康：`lcode doctor debug --id <session_id>`

## 常见问题矩阵

| 现象 | 常见原因 | 快速检查 | 处理建议 |
| --- | --- | --- | --- |
| `lcode list` 显示 `no sessions` | 当前无可见链接或链接为空 | `lcode link list` | 添加/修复链接，或用 `--local` 查看本地状态 |
| `session not found` | 会话 ID 不存在或作用域不对 | `lcode list`、`lcode --link <name> list` | 改用完整 ID，或切换 `--link`/`--local` |
| `session_id_ambiguous` | 短 ID 匹配多个会话 | `lcode list --format id` | 使用完整 ID |
| `stop`/`restart` 冲突 | 并发状态变化 | `lcode status --id <id>` | 重试或串行化生命周期操作 |
| `python_debugpy_unavailable` | Python 无 debugpy | `python3 -c "import debugpy"` | 安装 debugpy 或切换解释器 |
| `unsupported_dap_runtime` | runtime/backend 不支持 DAP | `lcode status --id <id>` | 使用支持的调试 runtime |
| `dap_error` 超时 | 调试通道未就绪或断开 | `lcode doctor debug --id <id> --json` | 先 `threads` 探测，必要时重启会话 |
| Node 调试不可用 | adapter 未解析成功 | `lcode doctor runtime --runtime node --json` | 设置 `LCODE_NODE_DAP_ADAPTER_CMD` |
| `project show` 空 | 当前作用域无项目元数据 | `lcode project list --all` | 使用全局/指定链接查看，或先 `project set` |
| 全局命令慢 | 链接过多/存在失效链接 | `lcode link list` | `lcode link prune --dry-run` 后执行 `lcode link prune` |

## 调试专项排查

### Python

```bash
lcode doctor runtime --runtime python --json
lcode debug --runtime python --entry app.py --cwd .
lcode dap threads --id <session_id>
lcode dap events --id <session_id> --max 50 --timeout-ms 1000
```

### Node

```bash
lcode doctor runtime --runtime node --json
lcode doctor debug --id <session_id> --json
```

若提示 adapter not found：

```bash
export LCODE_NODE_DAP_ADAPTER_CMD='["node","/path/to/js-debug/src/dapDebugServer.js"]'
```

## 生命周期批量操作建议

先预演：

```bash
lcode stop --all --status running --dry-run
```

再执行：

```bash
lcode stop --all --status running --yes
```

推荐添加失败控制参数：

```bash
lcode stop --all --status running --continue-on-error true --max-failures 0 --summary --yes
```

## 日志与性能建议

日志过滤：

```bash
lcode logs --id <session_id> --tail 500 --contains ERROR --exclude heartbeat
```

性能诊断：

```bash
lcode list --trace-time
lcode running --trace-time
```

清理与索引维护：

```bash
lcode cleanup --dry-run --status stopped
lcode link prune --dry-run
lcode link prune
```

## 自动化建议

- 脚本场景统一使用 `--json`。
- 只基于 `error` 字段分支，不依赖自然语言 `message`。
- 在 CI 中加入：

```bash
lcode doctor runtime --strict --json
```
