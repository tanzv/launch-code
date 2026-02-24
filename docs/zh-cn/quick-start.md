# 快速上手（中文）

本文档面向第一次使用 `lcode` 的开发者，给出最短路径的日常命令流程。

## 1. 安装

```bash
bash ./scripts/install.sh
```

如果你只想先安装 CLI：

```bash
bash ./scripts/install.sh --no-debug-deps
```

## 2. 启动一个运行会话

```bash
lcode start --runtime python --entry app.py --cwd .
```

## 3. 查看全局会话

```bash
lcode list
lcode running
```

说明：默认是全局视图，会聚合已注册链接下的会话。

## 4. 调试启动

```bash
lcode debug --runtime python --entry app.py --cwd . --host 127.0.0.1 --port 5678
```

查看调试会话基础信息：

```bash
lcode attach --id <session_id>
lcode inspect --id <session_id> --tail 80
```

## 5. 生命周期控制

```bash
lcode status --id <session_id>
lcode restart --id <session_id>
lcode stop --id <session_id>
```

批量操作示例：

```bash
lcode stop --all --status running --dry-run
lcode stop --all --status running --yes
```

## 6. 日志与诊断

```bash
lcode logs --id <session_id> --tail 200 --follow
lcode doctor runtime
lcode doctor debug --id <session_id>
```

JSON 自动化输出：

```bash
lcode --json list
lcode --json doctor runtime --strict --runtime python
```

## 7. 全局链接管理

注册项目链接：

```bash
lcode link add --name demo --path /path/to/workspace
```

查看链接：

```bash
lcode link list
```

指定链接执行命令：

```bash
lcode --link demo list
lcode --link demo project show
```

## 8. 常见推荐组合

日常开发：

```bash
lcode list
lcode inspect --id <session_id> --tail 80
lcode logs --id <session_id> --tail 200
```

调试排障：

```bash
lcode doctor runtime --json
lcode doctor debug --id <session_id> --json
lcode dap threads --id <session_id>
```

## 9. 下一步

- 中文总览：`docs/zh-cn/index.md`
- 命令大全：`docs/zh-cn/command-reference.md`
- JSON 错误码：`docs/zh-cn/json-error-codes.md`
- HTTP API：`docs/zh-cn/http-api.md`
- 运行时能力矩阵：`docs/zh-cn/runtime-debug-matrix.md`
- 故障排查：`docs/zh-cn/troubleshooting.md`
- 完整安装与排障：`docs/zh-cn/installation.md`
- Python 调试详解：`docs/python-debug-manual.md`
