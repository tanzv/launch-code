# launch-code 中文 README

`launch-code`（推荐命令：`lcode`）是一个面向本地开发的项目运行与调试管理 CLI。
默认以“全局链接视角”工作，可统一查看和管理多个工作区会话。

## 核心能力

- 运行/调试启动：`start`、`debug`
- 生命周期管理：`stop`、`restart`、`suspend`、`resume`
- 全局会话可见性：`list`、`running`
- 调试链路支持：`attach`、`dap`、`doctor debug`
- Go 调试模式：`debug`（默认）/`test`/`attach`
- 项目与配置管理：`project`、`config`

## 快速开始

1. 安装命令工具

```bash
bash ./scripts/install.sh
```

2. 注册工作区链接

```bash
lcode link add --name demo --path /path/to/workspace
```

3. 启动运行会话

```bash
lcode start --runtime python --entry app.py --cwd .
```

4. 启动调试会话

```bash
lcode debug --runtime python --entry app.py --cwd . --host 127.0.0.1 --port 5678
lcode debug --runtime go --entry ./cmd/app --cwd . --host 127.0.0.1 --port 43000
lcode debug --runtime go --go-mode test --entry ./pkg/service --cwd . --arg=-test.run --arg=TestServiceFlow
lcode debug --runtime go --go-mode attach --entry 12345 --cwd . --host 127.0.0.1 --port 43000
```

5. 查看与管理会话

```bash
lcode list
lcode running
lcode stop <session_id>
```

## 作用域模型（默认全局）

- 全局链接注册表：`$HOME/.launch-code/links.json`
- 工作区运行状态：`<workspace>/.launch-code/state.json`
- 默认命令行为：
  - `lcode list` / `lcode running`：跨链接聚合展示
  - 可用 `--link <name>` 指定单链接
  - 可用 `--local` 强制当前目录作用域

## 文档导航

- 总览入口：`docs/zh-cn/index.md`
- 安装指南：`docs/zh-cn/installation.md`
- 快速上手：`docs/zh-cn/quick-start.md`
- 命令参考：`docs/zh-cn/command-reference.md`
- JSON 错误码：`docs/zh-cn/json-error-codes.md`
- HTTP API：`docs/zh-cn/http-api.md`
- 能力矩阵：`docs/zh-cn/runtime-debug-matrix.md`
- 故障排查：`docs/zh-cn/troubleshooting.md`

## 建议

- 首次使用先执行 `lcode doctor runtime` 检查运行时环境。
- 全局链接较多时，定期执行 `lcode link prune` 清理无效链接。
- 自动化场景优先使用 `--json` 输出，便于脚本稳定处理。
