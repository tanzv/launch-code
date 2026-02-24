# launch-code 中文文档总览

本文档是 `lcode` / `launch-code` 的中文入口页，帮助你按场景快速定位资料。
如需一页式简版说明，可先看：`docs/zh-cn/README.md`。

## 文档地图

- 中文 README：`docs/zh-cn/README.md`
- 安装指南：`docs/zh-cn/installation.md`
- 快速上手：`docs/zh-cn/quick-start.md`
- 命令参考：`docs/zh-cn/command-reference.md`
- JSON 错误码：`docs/zh-cn/json-error-codes.md`
- HTTP API：`docs/zh-cn/http-api.md`
- 运行时与调试能力矩阵：`docs/zh-cn/runtime-debug-matrix.md`
- 故障排查：`docs/zh-cn/troubleshooting.md`
- Python 调试手册：`docs/python-debug-manual.md`

## 推荐阅读路径

### 新用户

1. `docs/zh-cn/installation.md`
2. `docs/zh-cn/quick-start.md`
3. `docs/zh-cn/command-reference.md`

### 自动化/平台集成

1. `docs/zh-cn/http-api.md`
2. `docs/zh-cn/json-error-codes.md`
3. `docs/zh-cn/troubleshooting.md`

### 调试专项

1. `docs/zh-cn/runtime-debug-matrix.md`
2. `docs/python-debug-manual.md`
3. `docs/zh-cn/troubleshooting.md`

## 版本与能力边界

- 默认作用域为全局链接视角（可用 `--local`/`--link` 覆盖）。
- Python/Node/Go 支持 debug 启动；Rust 当前仅 run 就绪。
- DAP 命令支持 Python/Go 调试会话；Node 依赖 adapter 可用性。
- 建议用 `lcode doctor runtime` 与 `lcode doctor debug` 做环境与会话健康检查。
