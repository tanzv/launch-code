# 运行时与调试能力矩阵（中文）

本文档说明不同 runtime 在 `run/debug/dap` 维度的能力边界，以及 `doctor runtime --strict` 的判定规则。

## 能力矩阵

| Runtime | `start` (run) | `debug` 启动 | DAP 命令 | `doctor runtime` |
| --- | --- | --- | --- | --- |
| Python | 支持 | 支持 | 支持（主要能力） | 支持 |
| Node | 支持 | 支持 | 依赖 adapter 可用性 | 支持 |
| Rust | 支持 | 启动受限（非完整调试后端） | 不完整/不可用 | 支持（run 维度） |

## strict 判定规则

执行：

```bash
lcode doctor runtime --strict
```

判定逻辑：

- `python`：`run_ready && debug_ready && dap_ready`
- `node`：`run_ready && debug_ready && dap_ready`
- `rust`：`run_ready`

若不满足 strict 条件，命令会返回：

- 错误码：`runtime_readiness_failed`
- 非零退出码（适合 CI 门禁）

## Python 建议

- 安装 `debugpy`：

```bash
python3 -m pip install --user debugpy
```

- 诊断：

```bash
lcode doctor runtime --runtime python --json
```

## Node 建议

Node adapter 解析顺序：

1. `LCODE_NODE_DAP_ADAPTER_CMD`
2. `PATH` 中的 `js-debug-adapter`
3. VSCode/Cursor 扩展脚本自动发现

推荐先做健康检查：

```bash
lcode doctor runtime --runtime node --json
```

若 adapter 不可用，可手动指定：

```bash
export LCODE_NODE_DAP_ADAPTER_CMD='["node","/path/to/js-debug/src/dapDebugServer.js"]'
```

## Rust 建议

- 当前以 run 场景为主。
- 可用 `doctor runtime` 进行工具链 readiness 检查。
- 调试链路建议关注后续版本增强，或结合外部调试工具。

## 组合诊断建议

环境层：

```bash
lcode doctor runtime --json
```

会话层：

```bash
lcode doctor debug --id <session_id> --json
```

若两者都通过，通常可进入 `dap` 命令级排查（`threads`/`stack-trace`/`events`）。
