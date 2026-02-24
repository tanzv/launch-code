# JSON 错误码与退出码（中文）

当命令使用 `--json` 时，错误信息会输出为结构化 JSON（通常在 stderr）。

## 错误输出结构

```json
{
  "ok": false,
  "error": "session_not_found",
  "message": "session not found: <id>"
}
```

## 退出码约定

| 退出码 | 含义 |
| --- | --- |
| `1` | 通用错误（I/O、状态、运行时等） |
| `2` | 参数/校验类错误（invalid\_\*、unsupported\_\*、strict 失败等） |
| `3` | 资源不存在/状态冲突类错误（session/profile/link 缺失等） |
| `4` | Python debugpy 不可用 |
| `5` | DAP 通道错误 |
| `6` | HTTP 服务错误 |

## 错误码全集

以下错误码来自 `src/error.rs`，可用于脚本稳定判断：

| 错误码 | 常见触发场景 | 常见处理 |
| --- | --- | --- |
| `io_error` | 文件/目录/权限异常 | 检查路径权限与磁盘状态 |
| `json_error` | JSON 解析或序列化失败 | 检查输入 JSON 格式 |
| `runtime_error` | 运行时层错误 | 检查 runtime 与 entry |
| `debug_error` | 调试初始化错误 | 检查 debug 配置与端口 |
| `config_error` | 配置处理失败 | 检查 config 输入 |
| `state_error` | 状态文件读写失败 | 检查 `.launch-code` 可读写性 |
| `stop_timeout` | stop 超时 | 增大 `--grace-timeout-ms` 或 `--force` |
| `process_error` | 进程层错误 | 检查 PID、信号、系统限制 |
| `session_not_found` | 会话不存在 | 先 `lcode list` 确认 ID |
| `session_id_ambiguous` | 会话短 ID 不唯一 | 使用完整 ID |
| `session_missing_pid` | 会话没有活动 PID | 检查是否已停止 |
| `session_missing_debug_meta` | 缺少 debug 元信息 | 用 `debug` 模式重启会话 |
| `session_missing_log_path` | 日志路径缺失 | 检查会话日志设置 |
| `session_state_changed` | 并发状态变更冲突 | 重试命令或串行执行 |
| `profile_not_found` | 配置不存在 | `lcode config list` 检查 |
| `profile_bundle_version_unsupported` | 导入 bundle 版本不兼容 | 使用受支持版本 |
| `profile_validation_failed` | 配置校验失败 | `lcode config validate` |
| `invalid_env_pair` | `--env` 非 `KEY=VALUE` | 修正参数格式 |
| `invalid_env_file_line` | env file 行格式错误 | 修正 env 文件 |
| `invalid_log_regex` | 日志 regex 非法 | 修正正则表达式 |
| `python_debugpy_unavailable` | Python 未安装 debugpy | `python -m pip install debugpy` |
| `unsupported_debug_runtime` | 该 runtime 不支持 debug | 调整 runtime 或模式 |
| `unsupported_dap_runtime` | 该 runtime/backend 不支持 dap | 使用支持的调试会话 |
| `http_error` | HTTP 服务处理异常 | 检查服务端日志 |
| `dap_error` | DAP 请求/通道失败 | 检查 `doctor debug` 与 adapter |
| `link_not_found` | 指定链接不存在 | `lcode link list` 或重新添加 |
| `invalid_link_path` | 链接路径不合法 | 修正路径并重建链接 |
| `invalid_start_options` | 启动参数组合非法 | 检查 `--help` 的约束 |
| `runtime_readiness_failed` | `doctor runtime --strict` 未通过 | 按建议补齐依赖 |
| `confirmation_required` | 全局批量操作缺少确认 | 增加 `--yes` |

## 脚本使用建议

1. 总是使用 `--json` 获取稳定字段。
2. 以 `error` 字段作为分支判断主键，不依赖文本 `message`。
3. 对 `session_id_ambiguous`、`session_state_changed` 设计可重试逻辑。
4. 在 CI 中可用 `doctor runtime --strict` 做环境门禁。
