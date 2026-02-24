# 安装指南（中文）

本文档提供 `launch-code` / `lcode` 的中文安装说明，包含一键安装、手动安装、升级、验证与常见问题。

## 安装后可用命令

- `lcode`（推荐）
- `launch-code`（兼容命令）

## 方案 A：一键安装（推荐）

```bash
bash ./scripts/install.sh
```

安装脚本会执行以下步骤：

1. 检查 Rust/Cargo，缺失时自动通过 `rustup` 安装。
2. 从当前仓库安装 CLI（`cargo install --path . --force`）。
3. 尝试安装调试依赖：
   - Python：`debugpy`
   - Node：js-debug adapter（自动探测/安装）

### 常用参数

只安装 CLI（跳过调试依赖）：

```bash
bash ./scripts/install.sh --no-debug-deps
```

要求调试依赖必须就绪（否则安装失败退出）：

```bash
bash ./scripts/install.sh --strict-debug-deps
```

查看帮助：

```bash
bash ./scripts/install.sh --help
```

## 方案 B：手动安装

```bash
cargo build
cargo install --path . --force
```

## 升级

```bash
git pull
bash ./scripts/install.sh
```

或：

```bash
cargo install --path . --force
```

## 安装验证

```bash
lcode --version
launch-code --version
lcode --help
lcode doctor runtime --json
```

## PATH 配置

如果安装后提示 `lcode: command not found`，把 Cargo bin 路径加入环境变量：

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

将该行写入你的 shell 配置文件（如 `~/.zshrc`）后重新打开终端。

## 调试依赖说明

### Python

```bash
python3 -m pip install --user debugpy
```

### Node Adapter 解析顺序

1. `LCODE_NODE_DAP_ADAPTER_CMD`（最高优先级，JSON 数组命令）
2. `PATH` 中的 `js-debug-adapter`
3. VSCode/Cursor 扩展里的 `dapDebugServer.js` 自动发现

手动指定示例：

```bash
export LCODE_NODE_DAP_ADAPTER_CMD='["node","/path/to/js-debug/src/dapDebugServer.js"]'
```

## 常见问题

### 1）`lcode` 命令找不到

- 检查安装是否成功。
- 检查 `$HOME/.cargo/bin` 是否在 `PATH`。
- 重新打开终端后执行 `lcode --version`。

### 2）Node adapter 安装失败（如 npm 镜像 404）

- CLI 安装仍可成功完成。
- 按提示手动设置 `LCODE_NODE_DAP_ADAPTER_CMD`。
- 用 `lcode doctor runtime --runtime node --json` 检查就绪状态。

### 3）Python 调试未就绪

- 为实际运行的 Python 解释器安装 `debugpy`。
- 通过以下命令确认：

```bash
python3 -c "import debugpy; print(debugpy.__version__)"
```

### 4）严格模式安装失败

使用 `--strict-debug-deps` 时，只要调试依赖不完整即会返回非零退出码。  
如果仅需要生命周期命令，可改用默认模式或 `--no-debug-deps`。

## 安装后健康检查

```bash
lcode doctor runtime
lcode doctor runtime --strict --runtime python --json
```

全局链路与会话可见性检查：

```bash
lcode link list
lcode list
lcode running
```
