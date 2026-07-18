# Token Usage

跨平台菜单栏 / 托盘应用，统计本机各 AI agent（Claude Code / Codex / ZCode / OpenCode / Qoder / Trae / Cursor / …）的 token 用量。本地优先，无云端。

> 状态：**V1 开发中** · 已完成脚手架 + 采集底座 + 存储层。

## 特性（V1 目标）

- **单一采集源**：所有工具的 token 统计经 [`tokscale`](https://www.npmjs.com/package/@tokscale/cli) 统一采集（含 Claude / Codex，无特殊路径）
- **单窗口**：菜单栏点击弹出 392px popover（Hero + 7 分段 + 全局 DAY / MONTH / TOTAL 时段切换），设置以居中 modal 弹出
- **7 分段视图**：总览 / 工具 / 模型 / 项目 / 趋势 / 会话 / 额度
- **额度**：厂商账号绑定（OAuth / API Key / Cookie 三类）→ 调厂商 API 读取，凭证一律存系统 keyring
- **本地存储**：SQLite（`daily_usage` / `sessions` / `collection_state` / `app_config`），支持任意时段查询
- **三平台**：macOS Apple Silicon / Windows x64 / Linux x64；产物 `.dmg` / `.msi` / `.AppImage + .deb + .rpm`

## 技术栈

| 层 | 选型 |
|----|------|
| 应用框架 | Tauri 2.x |
| 后端 | Rust（stable） |
| 前端 | Svelte 5 + Vite 8 + TypeScript |
| 存储 | SQLite（rusqlite bundled，WAL 模式） |
| 文件监控 | `notify` 6 |
| 凭证 | `keyring` |
| 采集 | tokscale（运行时下载 tarball，无 node 依赖） |

## 快速开始

**前置**：Rust stable（`rustup`）、Node 22+、macOS 需 Xcode Command Line Tools。

```bash
# 依赖
npm install

# 开发（vite :1420 + Rust 后端 + popover 窗口）
npm run tauri dev

# 构建（三平台安装包）
npm run tauri build
```

**注意**：中国大陆网络下，`registry.npmjs.org` 直连超时。可用镜像：

```bash
# 前端依赖
npm config set registry https://registry.npmmirror.com

# Rust crate 依赖 —— ~/.cargo/config.toml 配 rsproxy
[source.crates-io]
replace-with = "rsproxy-sparse"
[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

# tokscale 首启自动下载
TOKSCALE_REGISTRY=https://registry.npmmirror.com npm run tauri dev
```

## 项目结构

```
src/                    # Svelte 5 前端
src-tauri/              # Rust 后端
  src/
    main.rs / lib.rs    # 入口
    collector/          # tokscale spawn + 文件监控 + 采集调度（M1 完成）
    paths.rs            # 工具数据目录动态发现（tokscale clients --json）
    storage/            # SQLite schema + 迁移 + upsert（M2 进行中）
    commands/           # #[command] 暴露给前端（M3）
    query/              # SQL → 视图模型 VM（M2）
  examples/             # E2E 探针（e2e_install / e2e_clients / e2e_ingest_graph）
  tauri.conf.json       # 窗口 / 托盘 / 打包配置
  capabilities/         # Tauri 2 权限
docs/
  design.md             # 架构基线
  wireframe.html        # 最终 UI（392px popover 交互原型）
  plan.md               # 实施计划（WBS 31 任务，M0–M7）
.github/workflows/      # ci.yml（PR）+ release.yml（tag → 三平台产物）
```

## 常用命令

```bash
# 前端
npm run dev             # 仅 vite
npm run build           # 仅前端构建 → dist/
npm run check           # svelte-check 类型检查

# 后端（在 src-tauri/）
cargo test --lib               # 单元测试（当前 53 项）
cargo clippy -- -D warnings    # lint
cargo fmt --all -- --check     # 格式检查

# E2E 探针（真实网络 + 真实 tokscale）
cargo run --example e2e_install      # 下载 + 解压 + 运行 tokscale --version
cargo run --example e2e_clients      # 发现本机工具数据目录
cargo run --example e2e_ingest_graph # 落库真实一周数据
```

## 开发原则

- **TDD**：先写测试再实现（单元 + 集成 + E2E）
- **小文件**：200–400 行典型，800 上限；按 feature/domain 组织
- **不可变**：新对象替代原地修改
- **凭证不入库**：`.env` 仅示例，生产走 keyring
- **提交前**：`cargo fmt --check` + `cargo clippy -- -D warnings` + `cargo test`

## 里程碑进度

| 里程碑 | 状态 | 内容 |
|--------|------|------|
| M0 脚手架 + CI | ✅ | Tauri 2 + Svelte 5 + Vite，三平台 CI |
| M1 采集底座 | ✅ | tokscale 三层获取 + spawn / JSON 解析 + 路径动态发现 + 监控防抖 + 调度 |
| M2 存储 + 查询 | 🚧 | SQLite schema + upsert；查询 VM / 凭证 / 额度 adapter 待续 |
| M3 IPC + 前端骨架 | ⏳ | commands、popover 骨架、Tauri event 推送 |
| M4 分段视图 | ⏳ | 7 分段实装 |
| M5 设置 modal | ⏳ | 6 分区 |
| M6 打包验证 | ⏳ | 三平台产物 + 数据对账 + 性能 |
| M7 测试安全 | ⏳ | 补齐 80% 覆盖 + E2E + 安全审查 |

详见 [docs/plan.md](docs/plan.md)。

## 许可证

TBD
