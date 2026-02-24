# 命令参考（中文）

本文档给出 `lcode` 顶级命令、全局参数、常用模式和高频组合。

## 全局行为

- 默认是全局链接作用域（等价于 `--global`）。
- `--local` 强制使用当前工作区状态。
- `--link <name>` 将命令路由到指定链接工作区。
- 所有命令都支持 `--json` 输出结构化结果。

## 顶级命令一览

| 命令 | 作用 |
| --- | --- |
| `start` | 启动 run 会话 |
| `debug` | 启动 debug 会话 |
| `launch` | 从 `launch.json` 启动 |
| `attach` | 输出调试连接元数据 |
| `inspect` | 查看进程与日志尾部 |
| `logs` | 查看/跟随日志 |
| `stop` | 停止会话 |
| `restart` | 重启会话 |
| `suspend` | 挂起会话 |
| `resume` | 恢复会话 |
| `status` | 查看会话状态 |
| `list` (`ps`) | 列出会话 |
| `running` | 只列运行中的会话 |
| `cleanup` | 清理陈旧会话记录 |
| `config` | 配置文件管理（保存/运行/导入导出） |
| `project` | 项目元数据管理 |
| `link` | 全局链接管理 |
| `daemon` | 运行会话协调循环 |
| `serve` | 启动 HTTP 控制面 |
| `dap` | 发送 DAP 调试命令 |
| `doctor` | 诊断运行时与调试通道 |

## 关键命令族

### 启动命令

- `lcode start --runtime <python|node|rust> --entry <path> --cwd .`
- `lcode debug --runtime python --entry app.py --cwd . --host 127.0.0.1 --port 5678`
- `lcode launch --name "<LaunchName>" --mode <run|debug>`

环境变量合并顺序（`start` / `debug`）：

1. `--env-file`（按声明顺序，后者覆盖前者）
2. `--env KEY=VALUE`

### 会话生命周期命令

单会话：

- `lcode stop --id <session_id>`
- `lcode restart --id <session_id>`
- `lcode suspend --id <session_id>`
- `lcode resume --id <session_id>`
- `lcode status --id <session_id>`

位置参数简写：

- `lcode stop <session_id>`
- `lcode restart <session_id>`

多 ID：

- `lcode stop <id1> <id2>`
- `lcode restart <id1> <id2>`

批量：

- `lcode stop --all --status running --dry-run`
- `lcode stop --all --status running --yes`

批量控制参数（`stop/restart/suspend/resume`）：

- 过滤：`--status` `--runtime` `--name-contains`
- 失败策略：`--continue-on-error` `--max-failures`
- 计划控制：`--sort` `--limit` `--summary` `--jobs`

### 会话列表命令

- `lcode list`
- `lcode running`
- `lcode ps`

显示控制：

- `--format <table|compact|wide|id>`
- `--compact` / `--wide`
- `--short-id-len` `--no-trunc` `--no-headers` `-q`
- 监控模式：`--watch [INTERVAL] --watch-count <N>`

### 配置与元数据命令

`config` 子命令：

- `list` `show` `save` `delete` `run` `validate` `export` `import`

`project` 子命令：

- `show` `list` `set` `unset` `clear`

`link` 子命令：

- `list` `show` `add` `remove` `prune`

## 调试命令

### doctor

- `lcode doctor runtime`
- `lcode doctor runtime --runtime node --strict --json`
- `lcode doctor debug --id <session_id> --json`

### dap

常用子命令：

- `threads` `stack-trace` `scopes` `variables` `events`
- `breakpoints` `evaluate` `set-variable`
- `continue` `pause` `next` `step-in` `step-out`
- `disconnect` `terminate`
- `request` `batch`
- `adopt-subprocess`

## 运维命令

- `lcode cleanup --dry-run --status stopped`
- `lcode daemon --interval-ms 1000`
- `lcode serve --bind 127.0.0.1:8787 --token <token>`

## 快速查询建议

- 查看某命令完整参数：`lcode <command> --help`
- 查看某子命令完整参数：`lcode <command> <subcommand> --help`
- 机器可读输出：给命令追加 `--json`
