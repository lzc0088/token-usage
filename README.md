# Token Usage

跨平台菜单栏/托盘应用，实时统计本机各 AI 编码助手的 **token 用量与费用**。

支持 Claude Code · Codex · ZCode · OpenCode · Qoder · Trae · Cursor · Copilot · DeepSeek · Kimi · GLM · 等 18+ 工具。**数据 100% 本地存储，不上传云端。**

> 状态：**v1.0.0** · 全功能已交付，三平台安装包 CI 自动发布。

## 特性

| 模块 | 说明 |
|------|------|
| **总览** | 今日 I/O/缓存分项 + Top 3 工具/模型/额度，实时速率 |
| **工具 / 模型** | 按维度拆解用量占比，支持排序、展开详情 |
| **项目** | 自动按工作区/仓库分组，session JSONL cwd 回溯 |
| **趋势** | 近 7 天/当月折线图 + 日均/峰值统计 |
| **会话** | 按工具+项目排列，点击展开单会话模型明细 + 对话轮次 |
| **额度** | 17 家厂商余额/消耗窗口/到期时间，OAuth + API Key + Cookie 三类凭证 |
| **设置** | 基本/预览界面/窗口外观/采集追踪/账号额度 5 分区独立窗口 |
| **托盘** | 菜单栏图标 + 自定义显示（今日/累计 Tokens/成本）+ 右键菜单 |
| **采集** | 三种模式（实时/智能/定时间隔），会话保留/归档会话管理 |
| **更新** | 自动检查 GitHub Release 新版本，一键浏览器下载安装包 |

## 技术栈

| 层 | 选型 |
|----|------|
| 框架 | Tauri 2.x |
| 后端 | Rust（stable，rusqlite bundled，WAL 模式） |
| 前端 | Svelte 5 + Vite 8 + TypeScript |
| 存储 | SQLite（`daily_usage` / `sessions` / `quota_cache` / `app_config`） |
| 采集引擎 | [tokscale](https://www.npmjs.com/package/@tokscale/cli)（运行时下载 sidecar，无 node 依赖） |
| 凭证加密 | AES-256-GCM + 机器派生密钥 |
| 文件监控 | `notify`（inotify/FSEvents/ReadDirectoryChanges，含描述符耗尽回退） |

## 快速开始

**前置**：Rust stable、Node 22+、macOS 需 Xcode Command Line Tools。

```bash
npm install
npm run tauri dev    # 开发（vite :1420 + Rust 后端 + popover 窗口）
npm run tauri build  # 构建安装包
npm run check        # svelte-check 类型检查
```

**中国大陆网络**配置镜像：

```bash
# 前端依赖
npm config set registry https://registry.npmmirror.com

# Rust crate — ~/.cargo/config.toml:
[source.crates-io]
replace-with = "rsproxy-sparse"
[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

# tokscale 首启下载
TOKSCALE_REGISTRY=https://registry.npmmirror.com npm run tauri dev
```

## 常用命令

```bash
# 前端
npm run dev             # 仅 vite
npm run build           # 仅前端构建 → dist/
npm run check           # svelte-check 类型检查
npm test                # vitest

# 后端（在 src-tauri/ 下）
cargo test --lib               # 单元测试（当前 466 项）
cargo clippy -- -D warnings    # lint
cargo fmt --all -- --check     # 格式检查
```

## 项目结构

```
src/                        # Svelte 5 前端
  components/               # popover / segments / settings / common
  stores/                   # period / segment / modules
  lib/                      # api.ts, format.ts, i18n, quota-format, meta
src-tauri/                  # Rust 后端
  src/
    lib.rs / state.rs       # 入口 + 全局状态
    collector/              # tokscale 采集 → 调度 → snapshot
    storage/                # SQLite schema 迁移 + 读写
    commands/               # Tauri #[command]（查询/设置/额度/凭证/更新…）
    query/                  # 查询引擎（summary / breakdown / trends / sessions）
    quota/                  # 17 厂商额度适配器 + 调度
    auth/                   # 凭证加密存储
    config/                 # 用户配置 kv
    ui/                     # 窗口/托盘管理
  tauri.conf.json           # 窗口/托盘/打包（bundle.resources 注入 tokscale）
```

## CI / 发布

打 tag 自动构建三平台安装包并发布到 GitHub Release：

```
git tag v1.0.1 && git push origin v1.0.1
```

GitHub Actions 三平台构建（mac aarch64 / Windows x64 / Linux x64）→ 直接上传 GitHub Release。App 内「检查更新」自动检出。详见 `CLAUDE.md`。

## 开发原则

- **TDD**：先写测试再实现，覆盖 ≥80%
- **小文件**：200–400 行典型，800 上限
- **不可变**：新对象替代原地修改
- **凭证不入库**：`.env` 仅本地，CI 用 secrets
