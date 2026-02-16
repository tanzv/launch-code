# Python Debug Manual

本文档给出 `launch-code` 的 Python 调试完整操作流程，覆盖：

- 调试启动
- 断点设置（含条件断点）
- 单步执行（pause/next/step-in/step-out）
- 变量与调用栈查看
- 继续执行与会话结束
- 常见故障排查

## 1. Prerequisites

1. 安装 `debugpy`：

```bash
python -m pip install debugpy
```

2. 确认 `launch-code` 可执行：

```bash
cargo build
```

## 2. Start a Debug Session

```bash
launch-code debug --runtime python --entry app.py --cwd . --host 127.0.0.1 --port 5678 --subprocess true
```

启动成功后会输出类似：

```text
session_id=<id> pid=<pid> status=running debug_endpoint=127.0.0.1:5678
```

记录 `session_id`，后续命令都要使用。

## 3. CLI Debug Workflow (DAP CLI)

### 3.1 Set breakpoints

普通断点：

```bash
launch-code dap breakpoints --id <session_id> --path ./app.py --line 12 --line 34
```

条件断点 / 命中次数 / 日志断点：

```bash
launch-code dap breakpoints \
  --id <session_id> \
  --path ./app.py \
  --line 12 \
  --condition "x > 10" \
  --hit-condition "==2" \
  --log-message "value={x}"
```

### 3.2 Inspect runtime state

```bash
launch-code dap threads --id <session_id>
launch-code dap stack-trace --id <session_id> --thread-id 1 --levels 20
launch-code dap scopes --id <session_id> --frame-id 301
launch-code dap variables --id <session_id> --variables-reference 7001 --filter named --start 0 --count 20
launch-code dap evaluate --id <session_id> --expression "counter + 1" --frame-id 301 --context watch
launch-code dap set-variable --id <session_id> --variables-reference 7001 --name counter --value 42
```

异常断点（例如 raised/uncaught）：

```bash
launch-code dap exception-breakpoints --id <session_id> --filter raised --filter uncaught
```

### 3.3 Control execution

```bash
launch-code dap pause --id <session_id> --thread-id 1
launch-code dap next --id <session_id> --thread-id 1
launch-code dap step-in --id <session_id> --thread-id 1
launch-code dap step-out --id <session_id> --thread-id 1
launch-code dap continue --id <session_id> --thread-id 1
```

如果省略 `--thread-id`，工具会自动使用 `threads` 返回的第一个线程。

### 3.4 Read async events

```bash
launch-code dap events --id <session_id> --max 50 --timeout-ms 1000
```

### 3.5 Multiprocess: adopt child debug sessions

当 Python 程序创建子进程时，父会话会收到 `debugpyAttach` 事件。  
使用以下命令把子进程收编为新的 `session_id`，并自动完成 `initialize/attach/configurationDone`：

```bash
launch-code dap adopt-subprocess --id <session_id> --timeout-ms 2000 --max-events 50
```

成功后会输出 `child_session_id`。后续可直接用该子会话执行常规调试命令：

```bash
launch-code dap threads --id <child_session_id>
launch-code dap breakpoints --id <child_session_id> --path ./worker.py --line 20
launch-code dap continue --id <child_session_id>
```

### 3.6 End debug session

```bash
launch-code dap disconnect --id <session_id> --terminate-debuggee
launch-code dap terminate --id <session_id>
launch-code stop --id <session_id>
```

## 4. HTTP Debug Workflow

先启动控制平面：

```bash
launch-code serve --bind 127.0.0.1:8787 --token testtoken
```

以下请求都需要 Header：

```text
Authorization: Bearer testtoken
Content-Type: application/json
```

### 4.1 Breakpoints

```bash
curl -sS \
  -H "Authorization: Bearer testtoken" \
  -H "Content-Type: application/json" \
  -X POST \
  -d '{"path":"app.py","lines":[{"line":12,"condition":"x > 10","hitCondition":"==2","logMessage":"value={x}"}]}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/breakpoints
```

### 4.2 Debug controls

```bash
curl -sS -H "Authorization: Bearer testtoken" -H "Content-Type: application/json" \
  -X POST -d '{"threadId":1}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/pause

curl -sS -H "Authorization: Bearer testtoken" -H "Content-Type: application/json" \
  -X POST -d '{"threadId":1}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/next

curl -sS -H "Authorization: Bearer testtoken" -H "Content-Type: application/json" \
  -X POST -d '{"threadId":1}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/step-in

curl -sS -H "Authorization: Bearer testtoken" -H "Content-Type: application/json" \
  -X POST -d '{"threadId":1}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/step-out

curl -sS -H "Authorization: Bearer testtoken" -H "Content-Type: application/json" \
  -X POST -d '{"threadId":1}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/continue
```

### 4.3 Stack and variables

```bash
curl -sS -H "Authorization: Bearer testtoken" \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/threads

curl -sS -H "Authorization: Bearer testtoken" -H "Content-Type: application/json" \
  -X POST -d '{"filters":["raised","uncaught"]}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/exception-breakpoints

curl -sS -H "Authorization: Bearer testtoken" -H "Content-Type: application/json" \
  -X POST -d '{"expression":"counter + 1","frameId":301,"context":"watch"}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/evaluate

curl -sS -H "Authorization: Bearer testtoken" -H "Content-Type: application/json" \
  -X POST -d '{"variablesReference":7001,"name":"counter","value":"42"}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/set-variable

curl -sS -H "Authorization: Bearer testtoken" \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/dap/events?timeout_ms=1000&max=50
```

子进程收编（多进程调试）：

```bash
curl -sS -H "Authorization: Bearer testtoken" -H "Content-Type: application/json" \
  -X POST -d '{"timeout_ms":2000,"max_events":50}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/adopt-subprocess
```

返回的 `child_session_id` 可继续用于子进程调试 API，例如：

```bash
curl -sS -H "Authorization: Bearer testtoken" \
  http://127.0.0.1:8787/v1/sessions/<child_session_id>/debug/threads
```

如果需要完全自定义 DAP 请求，可用：

```bash
curl -sS -H "Authorization: Bearer testtoken" -H "Content-Type: application/json" \
  -X POST -d '{"command":"stackTrace","arguments":{"threadId":1,"levels":20}}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/dap/request
```

### 4.4 Disconnect and terminate

```bash
curl -sS -H "Authorization: Bearer testtoken" -H "Content-Type: application/json" \
  -X POST -d '{"terminateDebuggee":true,"suspendDebuggee":false}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/disconnect

curl -sS -H "Authorization: Bearer testtoken" -H "Content-Type: application/json" \
  -X POST -d '{"restart":false}' \
  http://127.0.0.1:8787/v1/sessions/<session_id>/debug/terminate
```

## 5. Troubleshooting

1. `PythonDebugpyUnavailable`
   - 含义：当前 Python 环境没有 `debugpy`。
   - 处理：执行 `python -m pip install debugpy`，或用 `--env PYTHON_BIN=/path/to/python` 指定已安装环境。

2. 端口冲突
   - 含义：请求端口被占用。
   - 处理：查看 `status` 输出中的 `debug_port` 与 `requested_debug_port`，使用实际分配端口连接。

3. `no threads reported by debug adapter`
   - 含义：调试器尚未停在可调试位置。
   - 处理：先设置断点并等待 `stopped` 事件，再执行 `next/step-in/variables`。

4. HTTP `401 unauthorized`
   - 含义：`Authorization` token 缺失或错误。
   - 处理：确认 Header 为 `Authorization: Bearer <token>`。

5. `no debugpyAttach event available; poll events and retry`
   - 含义：当前还没有收到子进程 attach 事件。
   - 处理：继续轮询 `dap events`（或 HTTP events），确认子进程已创建后再执行 `adopt-subprocess`。

## 6. Minimal Reproducible Demo

本节提供一个可直接复制的最小示例，用于验证断点命中、单步和变量读取。

### 6.1 Create demo file

你可以直接使用仓库内置示例：`docs/examples/python-debug-demo/app.py`。  
也可以手工创建一个 `app.py`，内容如下：

```python
import time


def compute(value: int) -> int:
    doubled = value * 2
    result = doubled + 3
    return result


def main() -> None:
    counter = 0
    while counter < 3:
        current = compute(counter)
        print(f"counter={counter} current={current}", flush=True)
        counter += 1
        time.sleep(0.5)

    time.sleep(30)


if __name__ == "__main__":
    main()
```

### 6.2 Start debug session

```bash
launch-code debug --runtime python --entry app.py --cwd .
```

从输出里记录 `session_id`。

### 6.3 Set breakpoint and wait for stop event

在 `current = compute(counter)` 这一行设置断点（示例按第 12 行）：

```bash
launch-code dap breakpoints --id <session_id> --path ./app.py --line 12
```

继续执行并轮询事件：

```bash
launch-code dap continue --id <session_id>
launch-code dap events --id <session_id> --max 20 --timeout-ms 2000
```

预期：返回 `event=stopped`，原因通常为 `breakpoint`。

### 6.4 Inspect stack and variables

```bash
launch-code dap threads --id <session_id>
launch-code dap stack-trace --id <session_id> --levels 20
```

从 `stackTrace` 结果中取 `frameId`，再查询作用域和变量：

```bash
launch-code dap scopes --id <session_id> --frame-id <frame_id>
launch-code dap variables --id <session_id> --variables-reference <variables_reference> --filter named
```

预期：可以看到 `counter`、`current`、`value` 等变量。

### 6.5 Step and resume

```bash
launch-code dap next --id <session_id>
launch-code dap step-in --id <session_id>
launch-code dap step-out --id <session_id>
launch-code dap continue --id <session_id>
```

### 6.6 End session

```bash
launch-code dap disconnect --id <session_id> --terminate-debuggee
launch-code stop --id <session_id>
```
