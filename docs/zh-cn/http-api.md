# HTTP API 使用说明（中文）

`lcode serve` 提供会话生命周期、项目元数据与调试控制的 HTTP 接口。

## 启动服务

```bash
lcode serve --bind 127.0.0.1:8787 --token testtoken
```

生产环境建议：

- 使用强随机 token。
- 使用 `--token-file` 管理 token。
- 仅监听内网地址并配合反向代理。

## 认证方式

所有请求需要 Header：

```http
Authorization: Bearer <TOKEN>
```

## 基础接口

- `GET /v1/health`
- `GET /v1/sessions`
- `GET /v1/sessions/{id}`
- `GET /v1/sessions/{id}/inspect?tail=50`
- `GET /v1/sessions/{id}/debug`

## 生命周期接口

- `POST /v1/sessions/{id}/stop`
- `POST /v1/sessions/{id}/restart`
- `POST /v1/sessions/{id}/suspend`
- `POST /v1/sessions/{id}/resume`
- `POST /v1/sessions/cleanup`

`cleanup` 请求体示例：

```json
{
  "dry_run": true,
  "statuses": ["stopped", "unknown"],
  "older_than_secs": 604800
}
```

## 项目元数据接口

- `GET /v1/project`
- `PUT /v1/project`
- `PATCH /v1/project`
- `DELETE /v1/project`

`PUT /v1/project` 示例：

```json
{
  "name": "launch-code",
  "languages": ["rust", "python"],
  "runtimes": ["python", "node"],
  "tools": ["debugpy"]
}
```

## DAP 原始接口

- `POST /v1/sessions/{id}/debug/dap/request`
- `GET /v1/sessions/{id}/debug/dap/events?timeout_ms=1000&max=50`

单请求示例：

```json
{
  "command": "threads",
  "arguments": {}
}
```

批量请求示例：

```json
{
  "batch": [
    { "command": "initialize", "arguments": { "clientID": "launch-code" } },
    { "command": "attach", "arguments": {} }
  ],
  "timeout_ms": 5000
}
```

说明：

- 上述 `attach` 示例主要适用于 Python/Node 的 attach 场景。
- Go 调试会话通常使用 `initialize` 后直接执行调试请求（例如 `threads`、`setBreakpoints`），不需要通用 `attach` 请求。

## 调试高层接口

- `GET /v1/sessions/{id}/debug/threads`
- `POST /v1/sessions/{id}/debug/breakpoints`
- `POST /v1/sessions/{id}/debug/exception-breakpoints`
- `POST /v1/sessions/{id}/debug/evaluate`
- `POST /v1/sessions/{id}/debug/set-variable`
- `POST /v1/sessions/{id}/debug/continue`
- `POST /v1/sessions/{id}/debug/pause`
- `POST /v1/sessions/{id}/debug/next`
- `POST /v1/sessions/{id}/debug/step-in`
- `POST /v1/sessions/{id}/debug/step-out`
- `POST /v1/sessions/{id}/debug/disconnect`
- `POST /v1/sessions/{id}/debug/terminate`
- `POST /v1/sessions/{id}/debug/adopt-subprocess`

`breakpoints` 请求体示例：

```json
{
  "path": "app.py",
  "lines": [
    { "line": 12, "condition": "x > 10", "hitCondition": "==2", "logMessage": "value={x}" }
  ]
}
```

## cURL 示例

读取会话列表：

```bash
curl -sS -H "Authorization: Bearer testtoken" \
  http://127.0.0.1:8787/v1/sessions
```

暂停调试线程：

```bash
curl -sS -X POST \
  -H "Authorization: Bearer testtoken" \
  -H "Content-Type: application/json" \
  -d '{"threadId":1}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/pause
```

读取 DAP 事件队列：

```bash
curl -sS -H "Authorization: Bearer testtoken" \
  "http://127.0.0.1:8787/v1/sessions/<session_id>/debug/dap/events?timeout_ms=1000&max=50"
```

## 常见问题

### 401 Unauthorized

- token 不正确或未传。
- 检查 `Authorization` 头是否为 `Bearer <TOKEN>`。

### 404 session_not_found

- 会话已不存在或被 cleanup。
- 先调用 `GET /v1/sessions` 确认可见会话。

### 5xx 或 dap_error

- 调试通道异常或 adapter 不可用。
- 建议执行 `lcode doctor debug --id <session_id> --json` 定位。
