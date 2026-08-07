<p align="right">
   <a href="./README.en.md">English</a> | <strong>简体中文</strong>
</p>

<div align="center">
    <img src="assets/app.png" alt="Token Usage logo" width="120">
    <h1>Token Usage</h1>
</div>

<p align="center">
    <em>一个菜单栏小工具，把 18+ 个 AI 编程助手的 Token 用量、费用、额度，全部装进 macOS 顶栏 / Windows 托盘。</em>
</p>

<p align="center">
    <a href="https://github.com/lzc0088/token-usage/releases"><img src="https://img.shields.io/github/v/release/lzc0088/token-usage?include_prereleases&style=flat-square&label=release&color=22c55e" alt="最新发布" /></a>
    <a href="https://github.com/lzc0088/token-usage/releases"><img src="https://img.shields.io/github/downloads/lzc0088/token-usage/total?style=flat-square&color=22c55e" alt="总下载量" /></a>
    <img src="https://img.shields.io/badge/Windows-10%2B-0078D4?style=flat-square" alt="Windows 10 或更新" />
    <img src="https://img.shields.io/badge/macOS-14%2B%20(Apple%20Silicon)-0A84FF?style=flat-square&logo=apple&logoColor=white" alt="macOS 14 或更新，Apple Silicon" />
    <img src="https://img.shields.io/badge/Linux-x64-64748b?style=flat-square&logo=linux&logoColor=white" alt="Linux x64" />
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-A855F7?style=flat-square" alt="许可证：MIT" /></a>
</p>

## Token Usage 是什么？

一款基于 **Tauri 2** 的跨平台菜单栏 / 托盘应用，实时统计本机各 AI 编程助手的 **Token 用量与费用**：

- 支持 **Claude Code · Codex · ZCode · OpenCode · Qoder · Trae · Cursor · Copilot · DeepSeek · Kimi · GLM** 等 18+ 工具
- 同步展示 **17 家厂商额度**（Claude / Codex / Cursor / OpenRouter / GLM / Kimi / Volcengine / Ollama / Minimax / …）
- **数据 100% 本地存储**，不依赖任何云服务、不上传 prompt / response

> 状态：**v1.0.3** · 全功能已交付，三平台安装包 CI 自动发布，应用内一键更新。

## 支持的工具

Token Usage 对 **Token 用量、账户额度、Session 明细** 分别提供支持（数据路径为工具在本机的默认存储位置）：

| 工具 | 数据路径 | Token 用量 | 额度 | Session 明细 |
|------|----------|:---:|:---:|:---:|
| **Claude Code** | `~/.claude/projects/` | ✅ | ✅ | ✅ |
| **Codex** | `~/.codex/sessions/` | ✅ | ✅ | ✅ |
| **ZCode / GLM** | `~/.zcode/projects/` | ✅ | ✅ | — |
| **OpenCode** | `~/.local/share/opencode/` | ✅ | — | ✅ |
| **Cursor** | `~/.config/tokscale/cursor-cache/` | ✅ | ✅ | — |
| **GitHub Copilot** | VS Code `workspaceStorage/*/chatSessions/` | ✅ | ✅ | — |
| **Kimi CLI / Kimi Code** | `~/.kimi/sessions/`、`~/.kimi-code/sessions/` | ✅ | ✅ | — |
| **DeepSeek** | DeepSeek API 密钥 | — | ✅ | — |
| **OpenRouter** | OpenRouter API 密钥 | — | ✅ | — |
| **Minimax** | Minimax API 密钥 | — | ✅ | — |
| **Volcengine** | 火山方舟 API 密钥 | — | ✅ | — |
| **Grok (xAI)** | `~/.grok/sessions/` | ✅ | ✅ | — |
| **Qoder** | Qoder dashboard cookie | — | ✅ | — |
| **Ollama Cloud** | Ollama Cloud cookie | — | ✅ | — |
| **iFlytek** | 星河 API 凭证 | — | ✅ | — |
| **Stepfun** | Stepfun API 密钥 | — | ✅ | — |
| **MiMo Code** | `~/.local/share/mimocode/mimocode.db` | ✅ | ✅ | — |
| **Qwen CLI** | `~/.qwen/projects/` | ✅ | — | — |
| **Trae** | （由 tokscale 适配） | ✅ | — | — |
| **Hermes / Zed / Cline / Kiro / CodeBuddy / WorkBuddy / Proma / Pi** 等 | （详见 tokscale 支持列表） | ✅ | — | — |

## 界面展示

<table>
<tr>
<td width="290" align="center"><img src="assets/home-view.png" width="250" alt="总览视图"><br><sub>总览——今日 I/O/缓存分项 + Top 3 工具/模型/额度，实时速率</sub></td>
<td width="290" align="center"><img src="assets/tools-view.png" width="250" alt="工具视图"><br><sub>工具——按维度拆解用量占比，支持展开缓存命中明细</sub></td>
<td width="290" align="center"><img src="assets/models-view.png" width="250" alt="模型视图"><br><sub>模型——跨工具汇总每个模型的用量与成本</sub></td>
</tr>
<tr>
<td width="290" align="center"><img src="assets/quotas-view.png" width="250" alt="额度视图"><br><sub>额度——17 家厂商余额/消耗窗口/到期时间，三类凭证</sub></td>
<td width="290" align="center"><img src="assets/sessions-view.png" width="250" alt="会话视图"><br><sub>会话——按工具+项目排列，点击展开单会话模型明细 + 对话轮次</sub></td>
<td width="290" align="center"><img src="assets/trends-view.png" width="250" alt="趋势视图"><br><sub>趋势——近 7 天/当月折线图 + 日均/峰值统计</sub></td>
</tr>
<tr>
<td width="290" align="center"><img src="assets/projects-view.png" width="250" alt="项目视图"><br><sub>项目——按工作区/仓库自动分组，session JSONL cwd 回溯</sub></td>
<td width="290" align="center"><img src="assets/settings-general.png" width="250" alt="设置 常规"><br><sub>设置——基本/预览界面/窗口外观/采集追踪/账号额度 5 分区独立窗口</sub></td>
<td width="290" align="center"><img src="assets/tray-popover.png" width="250" alt="菜单栏 popover"><br><sub>菜单栏——图标 + 自定义显示（今日/累计 Tokens/成本）+ 右键菜单</sub></td>
</tr>
</table>

## 为什么用 Token Usage？

大多数用量监控工具是为**单设备**设计的、要**先在网页上登录**。Token Usage 是 **Tauri 2 + 本地优先** 的菜单栏小工具：

- **本机直读**：不连后端、不上传 prompt / response，所有数据 100% 留在你的电脑
- **菜单栏 / 托盘常驻**：不抢主窗口，点击即看今日/累计用量与额度余量
- **应用内一键更新**：检测到新版本→ 下载 → 验签 → 安装 → 提示重启，不跳出浏览器
- **采集 / 额度两套独立通道**：token 走 tokscale、额度走凭证→厂商 API，互不污染

## 功能特性

### 用量追踪

- **实时 Token 追踪** — Claude Code、Codex、ZCode、OpenCode、Qoder、Trae 等 18+ AI 工具，每轮对话后 UI 在数秒内刷新
- **单 Session 明细** — 打开单个 session，逐条提问拆解 token、模型与用到的工具
- **缓存命中统计** — 点任一工具/模型，展开输入 token（缓存命中与未命中）、输出 token 的详细分类及命中率
- **成本与币别** — Token 数旁附带成本，可用 USD / CNY 切换显示，汇率每日自动更新，亦可在设置中手动覆写
- **项目 / 工作区分组** — 从 session JSONL 的 `cwd` 反推项目名，按工作区或仓库聚合
- **今日速率读出** — 实时算 output tokens/s（模型忙时）或总 tokens/min（消耗速率）

### 额度、趋势与设置

- **AI 工具额度检测** — 17 家厂商的 session、每周、账单与 credits 窗口，包括 DeepSeek 预付余额、OpenRouter 用量上限、Minimax Token Plan、Volcengine 火山方舟 Coding Plan
- **多账号 / 多凭证** — 同一厂商可配置多账号；OAuth（Copilot / Codex）、API Key、Cookie 三类凭证并存，凭证用 AES-256-GCM + 机器派生密钥加密落盘
- **使用趋势** — 近 7 天 / 当月折线图，日均/峰值统计
- **自动更新** — 应用内一键下载 + 验签 + 安装 + 提示重启；支持强制检查、1h 缓存冷却
- **5 分区设置** — 基本 / 预览界面 / 窗口外观 / 采集追踪 / 账号额度，每个分区独立窗口

### 采集与界面

- **三种采集模式** — 实时（file-watch 推送）/ 智能（10min 间隔 + 活动感知）/ 定时间隔
- **会话保留与归档** — 工具卸载 / 旧 session 被清掉后，仍可在本地继续统计；归档会话可在设置中查看/清空
- **菜单栏与托盘** — macOS 菜单栏 / Windows 系统托盘 / Linux 系统托盘；今日 / 累计 Tokens / 成本 / 仅图标 多种显示模式
- **窗口模式** — popover（默认）、固定、置顶三种；支持全局热键唤起
- **主题与外观** — 暗 / 亮 / 跟随系统；玻璃透明度与模糊度可调；完全透明窗口模式
- **i18n** — 简体中文 / 英文界面

## 安装

从 [GitHub Releases](https://github.com/lzc0088/token-usage/releases) 下载：

- **macOS (Apple Silicon)** — `.dmg`，已签名 + 已公证（Developer ID）
- **Windows 10 / 11** — NSIS `.exe` 安装包
- **Linux x64** — `.AppImage`

打包版会自动检查 GitHub Releases。设置 → 常规 中有更新时，可点击 **立即更新** 在应用内完成下载与安装。打 tag 即可触发三平台构建并发布到 GitHub Release：

```bash
git tag v1.0.3 && git push origin v1.0.3
```

GitHub Actions 在三平台 runner（mac aarch64 / Windows x64 / Linux x64）上构建，产物直接上传到 GitHub Release，应用内「检查更新」自动检出。

> **中国大陆网络**：首启 tokscale 下载、GitHub Release CDN 可能较慢。应用会自动读取系统代理（macOS `scutil` / Windows 注册表 / Linux `HTTPS_PROXY`），也可在 `TOKSCALE_REGISTRY=https://registry.npmmirror.com npm run tauri dev` 启动时指定 npm 镜像。

### 首次启动

应用启动后默认进入「菜单栏 / 托盘常驻」模式，**不需要任何账号或注册**——打开即用。额度查询需要在「设置 → 账号额度」中按厂商填入对应凭证（API Key / Cookie / OAuth）。

## 从源码构建

```bash
# 前置：Rust stable、Node 22+、macOS 需 Xcode Command Line Tools
npm install
npm run tauri dev    # 开发（vite :1420 + Rust 后端 + popover 窗口）
npm run tauri build  # 三平台安装包
npm run check        # svelte-check 类型检查
```

### 常用命令

```bash
# 前端
npm run dev             # 仅 vite
npm run build           # 仅前端构建 → dist/
npm run check           # svelte-check
npm test                # vitest

# 后端（在 src-tauri/ 下）
cargo test --lib               # 单元测试
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
    utils/                  # proxy / http / file / log
  tauri.conf.json           # 窗口/托盘/打包（bundle.resources 注入 tokscale）
```

## 数据存储

应用状态保存在系统的用户数据目录——卸载时一并删除该目录即可完整移除。

| 平台 | 路径 |
|------|------|
| macOS | `~/Library/Application Support/token-usage/` |
| Windows | `%APPDATA%/token-usage/` |
| Linux | `~/.config/token-usage/` |

## 工作原理

```text
菜单栏 / 托盘应用
  ├─ 采集线程 ──▶ tokscale (sidecar) ──▶ ~/.claude, ~/.codex, ~/.zcode, ...
  │                 │                       （parse JSONL → 聚合 token）
  │                 ▼
  │              SQLite (WAL) ◀─── query/ 引擎 ───▶ 前端 Svelte 5
  │              ├ daily_usage
  │              ├ sessions
  │              ├ quota_cache
  │              └ app_config
  │
  └─ 额度线程 ──▶ 凭证（keyring/加密 KV）──▶ 厂商 API
                   Claude / Codex / Cursor / OpenRouter / Volcengine / ...
                   （OAuth 走 device code，Cookie 走 dashboard，API Key 走 balance）
```

采集与额度两套线程独立运行；所有网络请求读取系统代理（macOS / Windows / Linux）。

## 隐私

Token Usage 在本机处理使用日志，**不向任何远程服务发送 prompt / response / 源代码**。网络访问仅用于：

- 应用内更新检查（GitHub Releases）
- 文档所述的额度查询（厂商官方 API，凭证由你本地提供）

应用不会：

- 上传你的 prompt、回复、代码或文件内容
- 收集遥测、统计、崩溃报告
- 调用任何云端的用量聚合服务

## 致谢

- [tokscale](https://github.com/junhoyeo/tokscale) — 日志解析与 token 计算
- [tauri](https://github.com/tauri-apps/tauri) — 跨平台原生窗口与 IPC
- [CodexBar](https://github.com/steipete/CodexBar) — 厂商额度研究参考

## 许可证

[MIT](LICENSE) © [Ze Chuan](https://github.com/lzc0088)
