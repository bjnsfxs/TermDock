# Plan.md — AI CLI 多终端管理器（Windows Host Daemon + 多端客户端）

> 目标：在 **Windows 宿主机**上运行一个守护进程（Daemon），统一管理多个 AI CLI 进程（启动/停止/重启/监控/交互），并通过 **HTTP API + WebSocket** 让 **Windows 桌面端**与**移动端**都能实时查看状态并接管终端交互。

---

## 0. 关键决策（固定，不在实现中反复摇摆）

### 0.1 架构
- **Daemon（服务端）**：常驻 Windows（可作为前台进程或服务/计划任务启动）
  - 负责：实例配置管理、进程生命周期、PTY（ConPTY）交互、日志与指标采集、鉴权、对外 API
- **Client（客户端）**：
  - **桌面端**：Tauri v2 + React（内置 xterm.js）
  - **移动端**：优先交付 **PWA（同一套 React UI）**；随后可选做 **Tauri v2 Android** 打包
  - 负责：UI、配置编辑、终端渲染、与 Daemon 通信

> 说明：PWA 作为移动端 MVP 不需要 iOS/macOS 构建环境；后续若要 iOS，需要 macOS + Xcode。

### 0.2 技术栈（推荐最终选型）
- Daemon：Rust（tokio + axum） + SQLite（sqlx） + portable-pty（ConPTY） + sysinfo
- Terminal UI：xterm.js（Fit Addon + 可选 WebGL Addon）
- 客户端 UI：React + TypeScript + TanStack Query（或 SWR） + Zustand（可选）
- API 描述：OpenAPI（REST 部分），并生成 TS client 类型

### 0.3 最小可交付（MVP）必须满足
- 创建/编辑/删除 “CLI 实例（Instance）”
- 启动/停止/重启 Instance
- Dashboard 网格展示状态 + CPU/Mem（粗略即可）
- 进入 Terminal 页面后能：
  - 实时看到 ANSI 彩色输出
  - 键盘输入可写入到 CLI stdin（通过 PTY）
  - 支持 resize（窗口改变列/行）
- 有 **Token 鉴权**（至少 Bearer Token）
- Daemon 默认仅监听 `127.0.0.1`，显式开启后才允许局域网访问（避免误暴露）

---

## 1. 仓库结构（Monorepo）

```
repo/
  daemon/                 # Rust daemon
    src/
    migrations/
    Cargo.toml
  client/                 # Tauri v2 + React (desktop)
    src-tauri/
    src/
    package.json
  web/                    # PWA（复用同一套 React UI，或用 client 的 web 模式）
    src/
    package.json
  packages/
    api-client/           # openapi-typescript 生成的 TS 类型 + 封装
    ui/                   # 可选：共享组件（Terminal、Cards、Forms）
  docs/
    API.md
    ARCHITECTURE.md
    SECURITY.md
```

> 实际落地时可以先做 `daemon/` + `client/` 两个目录；`web/` 与 `packages/` 视进度再拆。

---

## 2. 数据模型（Instance）

### 2.1 Instance（持久化）
字段建议（SQLite）：
- `id` TEXT (uuid) PK
- `name` TEXT NOT NULL
- `enabled` INTEGER NOT NULL DEFAULT 1
- `command` TEXT NOT NULL               # 例如 "ollama" / "python" / "aider"
- `args_json` TEXT NOT NULL             # JSON array string
- `cwd` TEXT NULL
- `env_json` TEXT NOT NULL              # JSON object string（敏感值后续可加密）
- `use_pty` INTEGER NOT NULL DEFAULT 1  # 默认用 PTY
- `config_mode` TEXT NOT NULL           # "none" | "path" | "inline"
- `config_path` TEXT NULL               # config_mode="path" 时
- `config_filename` TEXT NULL           # config_mode="inline" 时，写入 instance_dir 下文件名
- `config_content` TEXT NULL            # config_mode="inline" 时
- `restart_policy` TEXT NOT NULL        # "never" | "on-failure" | "always"
- `auto_start` INTEGER NOT NULL DEFAULT 0
- `created_at` TEXT NOT NULL (ISO8601)
- `updated_at` TEXT NOT NULL (ISO8601)

### 2.2 Runtime（内存态，不落库）
- `status`: "stopped" | "starting" | "running" | "stopping" | "exited" | "error"
- `pid`: u32?
- `started_at`: timestamp?
- `exit_code`: i32?
- `cpu_percent`: f32?
- `mem_bytes`: u64?
- `clients_attached`: u32
- `control_owner`: client_id?（可选：实现“接管锁”）

---

## 3. Daemon 设计

### 3.1 目录与运行时数据
- 数据根目录（Windows 推荐）：`%APPDATA%/ai-cli-manager/`
  - `db.sqlite`
  - `instances/<id>/config.yaml`（inline 模式写入）
  - `logs/daemon.log`
  - `logs/instances/<id>.log`（可选：输出落盘）
  - `secrets.json`（可选：token 等）

### 3.2 进程与 PTY 管理（核心）
每个 running instance 都维护：
- `pty_master: Box<dyn MasterPty>`
- `pty_writer: Box<dyn Write + Send>`（或持有 master）
- `child: Box<dyn Child>`
- `broadcast_tx: tokio::sync::broadcast::Sender<Vec<u8>>`（输出广播）
- `control_lock: tokio::sync::Mutex<Option<client_id>>`（可选）

读循环（线程/阻塞任务）：
- 从 PTY master read bytes
- 写入：
  - broadcast（给所有 websocket）
  - ring buffer（用于新客户端 attach 时补发最近内容）
  - optional：落盘日志（append）

写入 stdin：
- websocket 收到 binary data -> 写入 PTY

resize：
- websocket 收到 JSON `{type:"resize", cols, rows}` -> `master.resize(...)`

结束：
- child.wait() -> 更新状态 -> 广播 event -> 关闭广播

> 注意：ConPTY/伪控制台的 I/O 建议单独线程服务，避免死锁（参考 Microsoft 文档理念）。

### 3.3 API（REST）
建议前缀：`/api/v1`

- `GET /health` -> `{status:"ok", version, uptime}`
- `GET /api/v1/instances`
- `POST /api/v1/instances`
- `GET /api/v1/instances/{id}`
- `PUT /api/v1/instances/{id}`
- `DELETE /api/v1/instances/{id}`

控制：
- `POST /api/v1/instances/{id}/start`
- `POST /api/v1/instances/{id}/stop`
- `POST /api/v1/instances/{id}/restart`

配置内容：
- `GET /api/v1/instances/{id}/config`（inline/path 的统一视图）
- `PUT /api/v1/instances/{id}/config`

日志/输出（可选）：
- `GET /api/v1/instances/{id}/output?tail=2000`（返回最近 N bytes/base64）

服务设置（可选）：
- `GET /api/v1/settings`
- `PUT /api/v1/settings`

鉴权/配对：
- `POST /api/v1/auth/pair` -> 生成/刷新 token（需要本机批准的策略见 SECURITY）

### 3.4 WebSocket
- **终端 I/O：** `WS /ws/v1/term/{id}`
  - Server -> Client：
    - binary：PTY 输出 bytes
    - text(JSON)：事件，如 `{type:"status", status:"running"}`、`{type:"error", message:"..."}`
  - Client -> Server：
    - binary：stdin bytes（来自 xterm onData）
    - text(JSON)：控制消息
      - resize: `{type:"resize", cols: 120, rows: 30}`
      - request_tail: `{type:"tail", bytes: 8192}`（服务端从 ring buffer 补发）

- **全局事件：** `WS /ws/v1/events`
  - 推送 instance 状态变化、指标更新、daemon 通知
  - 用于 Dashboard 实时刷新，减少轮询

### 3.5 鉴权（最低安全线）
- 所有 `/api/v1/*` 与 `/ws/v1/*` 都要求 `Authorization: Bearer <token>`
- Token 存储：
  - MVP：存明文到 `secrets.json`（仅本机可读）
  - v1：Windows DPAPI/加密存储（后续）

- 监听地址策略：
  - 默认：`127.0.0.1:PORT`
  - 需要局域网：显式设置 `--bind 0.0.0.0` 并在 UI 提示风险

---

## 4. Client（桌面端）设计

### 4.1 页面/路由
- `/` Dashboard（网格）
- `/instances/new` 新建
- `/instances/:id/edit` 编辑
- `/instances/:id/term` 终端接管
- `/settings` 服务器地址、token、局域网开关、生成二维码（仅 UI）

### 4.2 Dashboard（Grid）
每张卡片显示：
- name
- status 指示灯
- pid（若有）
- CPU%、Mem
- Buttons：Start / Stop / Restart / Open Terminal / Edit

数据来源：
- 初次进入：REST `GET /instances`
- 后续：订阅 `/ws/v1/events`（指标/状态推送）
  - 若 websocket 不可用，退化到 2s 轮询

### 4.3 Terminal View（xterm.js）
- 初始化：
  - `Terminal({ cursorBlink: true, convertEol: false })`
  - 加载 FitAddon，进入页面后 `fit()`
- websocket：
  - `binaryType = "arraybuffer"`
  - onmessage：
    - ArrayBuffer -> TextDecoder -> `term.write(text)`
    - string -> JSON parse -> 处理 status/error/tail 等
- 输入：
  - `term.onData(data => ws.send(TextEncoder().encode(data)))`
- resize：
  - 监听 fit-addon 或 window resize，节流 100ms，发送 JSON resize

---

## 5. 移动端（两步走）

### 5.1 MVP：PWA
- 复用同一套 React UI（与 desktop 几乎一致）
- 通过设置页输入：
  - Daemon 地址（如 `http://192.168.1.10:8765`）
  - Token
- 提供 “生成二维码”：
  - Desktop 显示二维码（包含 address + token）
  - Mobile 用相机扫码自动填充

### 5.2 后续：Tauri v2 Android
- 将 PWA UI 打包为 Tauri mobile
- 注意：
  - Android 网络权限
  - WebSocket 在移动网络的重连策略
  - 软键盘与 xterm 输入体验优化（比如把输入 focus 固定到 term）

---

## 6. 里程碑拆解（Codex 按顺序执行）

> 原则：每个里程碑都 **可运行**、**可验收**。不要一次写完所有功能。

### M0 — 初始化与规范
**目标：** repo 可构建、可运行空壳
- [ ] 初始化 monorepo（pnpm workspace 或 npm workspaces）
- [ ] `daemon`：Rust 项目 + `axum` 最小 server（/health）
- [ ] `client`：Tauri v2 + React 跑起来，能请求 /health 并展示结果
- [ ] 统一格式化与 lint：
  - Rust: rustfmt + clippy
  - TS: eslint + prettier
- [ ] 基础 CI（可选）：lint + test

验收：
- `cargo run -p daemon` 启动后，`GET /health` OK
- `pnpm dev` 启动客户端可看到 health 状态

---

### M1 — SQLite + Instance CRUD（无进程）
**目标：** 能增删改查实例配置
- [ ] 引入 `sqlx` + sqlite + migrations
- [ ] 定义 instances 表 + migrations
- [ ] 实现 REST：
  - list/create/get/update/delete
- [ ] 输入校验（command/name 不能为空；args/env 必须是合法 JSON）
- [ ] client 端：
  - Dashboard 列表
  - New/Edit 表单（基础字段）

验收：
- 创建实例后能在 Dashboard 看到
- 重启 daemon 后数据仍在

---

### M2 — 进程管理（先不接 PTY，只用 pipes）
**目标：** start/stop/restart + 记录 pid + 捕获 stdout
- [ ] `ProcessManager`：
  - spawn（tokio::process::Command）
  - stop（优雅 -> 超时 -> kill）
  - restart
- [ ] 输出捕获：
  - stdout/stderr 合并，写入 ring buffer & optional 落盘
- [ ] REST 控制接口生效
- [ ] client 端按钮可用，状态能刷新（轮询即可）

验收：
- 能启动一个简单命令（如 `python -c "print('hi'); input()"`）
- stop 能终止进程

---

### M3 — PTY（portable-pty）+ 单实例终端 WS
**目标：** xterm.js 可真实交互（ANSI 保真）
- [ ] 引入 portable-pty
- [ ] start 时根据 `use_pty` 决定：
  - PTY：portable-pty openpty + spawn
  - 非 PTY：走 pipes（保留）
- [ ] WS `/ws/v1/term/{id}`：
  - 输出 binary push
  - 输入 binary write
  - resize JSON
  - tail JSON（新连接补历史）
- [ ] client Terminal 页面接入 xterm.js

验收：
- 在 Terminal 页面输入能驱动 CLI 行为
- ANSI 色彩/进度条不丢失（至少不被破坏）
- 断开 websocket 再连能看到 tail

---

### M4 — 全局事件 WS + Dashboard 实时刷新 + Metrics
**目标：** Grid 不靠频繁轮询也能更新
- [ ] `/ws/v1/events` 推送：
  - instance status change
  - metrics update（每 1s 或 2s）
- [ ] sysinfo 采集 pid CPU/Mem
- [ ] client：
  - 订阅 events 更新卡片
  - websocket 断线重连（指数退避）

验收：
- start/stop 后 dashboard 状态秒级更新
- CPU/Mem 有变化

---

### M5 — 安全与局域网访问
**目标：** 可安全给手机用
- [ ] Bearer Token 中间件（REST + WS）
- [ ] daemon 配置：
  - bind address（默认 127.0.0.1）
  - port
  - token（首次启动生成）
- [ ] client Settings 页：
  - 展示 token（复制）
  - 展示二维码（address + token）
- [ ] 文档：SECURITY.md（风险、建议、如何开启局域网）

验收：
- 无 token 请求会 401
- 有 token 可正常操作
- 手机浏览器访问 PWA 能连接到 daemon（局域网）

---

### M6 — 打包与交付
**目标：** 可给真实用户安装使用
- [ ] daemon：release 构建 + 默认配置目录
- [ ] desktop：tauri build
- [ ] 可选：daemon 自启动（计划任务）
- [ ] README：安装、启动、连接手机、常见问题

验收：
- 一台干净 Windows 机器按 README 可跑通完整流程

---

## 7. 测试策略（最低要求）
- daemon：
  - 单元测试：配置校验、模板替换、DB CRUD
  - 集成测试：启动 daemon -> 调用 REST -> 验证状态
- client：
  - 最少：TypeScript 类型检查 + lint
  - 可选：Playwright e2e（打开 dashboard，创建实例，进入终端）

---

## 8. 风险清单与对策
- **ConPTY 版本要求**：Windows 10 1809+ 才稳定（旧系统不支持）
  - 对策：启动时检测 OS build，给出明确错误提示
- **不同 CLI 行为差异**：有些 CLI 不需要 PTY，有些必须 PTY
  - 对策：每实例提供 `use_pty` 开关
- **网络暴露风险**：daemon 监听 0.0.0.0 可能被同网段访问
  - 对策：默认 localhost；强制 token；文档强调；后续可加 TLS
- **移动端输入体验**：软键盘、IME、快捷键
  - 对策：xterm 配置 + 额外输入栏（可选）

---

## 9. 未来扩展（不进 MVP）
- 多用户/权限（viewer/controller）
- 录屏/会话回放（保存输出、导出）
- 插件系统（预置 Ollama/Aider/Claude CLI 模板）
- TLS + 自签证书/证书导入
- mDNS 发现 + “一键发现主机”
