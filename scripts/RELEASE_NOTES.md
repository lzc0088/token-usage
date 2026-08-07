# English

## What's new

### Tracking & sessions
- **18+ AI coding tools:** Real-time token tracking for Claude Code, Codex, ZCode, OpenCode, Qoder, Trae, Cursor, Copilot, Kimi, GLM, DeepSeek, Grok, Qwen, MiMo, and more — UI refreshes within seconds of each turn.
- **Per-session detail:** Open a session to see tokens per prompt, models and tools used, and per-reply input/output/cache-read/cache-write split.
- **Cache-hit statistics:** Click any tool or model to expand cache-hit vs cache-miss breakdown with hit-rate percentages.
- **Project / workspace grouping:** Sessions auto-grouped by project name (derived from the tool's `cwd`), aggregatable by workspace or repo.
- **Usage trends:** Last 7 days / current month line chart with daily average and peak.
- **Live rate readout:** Real-time output tokens/s (model-busy time) or total tokens/min (burn rate), configurable.
- **Cost & currency:** USD / CNY display with daily auto-updated exchange rate (manual override available).

### Quotas
- **17 vendor quotas:** Claude, Codex, Cursor, OpenRouter, GLM, Kimi, Volcengine, Ollama, Minimax, DeepSeek, iFlytek, Stepfun, Qoder, Copilot, Grok, MiMo — session / weekly / billing / credits windows per vendor.
- **Multi-account & multi-credential:** Multiple accounts per provider; OAuth (Copilot / Codex), API Key, and Cookie credentials co-exist, encrypted at rest (AES-256-GCM + machine-derived key).

### Collection
- **Three collection modes:** Live (file-watch push), Smart (10min interval, activity-gated), Fixed interval.
- **Session retention & archive:** Keep daily tool/model stats locally even after the source tool prunes or is uninstalled; view / clear archived sessions in Settings.

### Interface
- **Menu bar & system tray:** macOS menu bar, Windows / Linux system tray. Customizable readout: today / total tokens or cost, or icon-only. Context menu for quick actions.
- **Popover window:** Click the tray icon to show a 458px popover with the full dashboard — period switcher, overview, tools/models breakdown, quotas, trends, sessions, projects.
- **Settings (5 separate windows):** General (language, theme, updates) / Preview / Window (popover/pinned/always-on-top, global hotkey) / Collection (modes, tracked tools, archive) / Quotas (vendors, credentials, refresh interval).
- **Appearance:** Dark / light / follow-system themes; adjustable glass opacity and blur; fully transparent window mode.
- **Global hotkey:** Recordable shortcut to summon the popover from anywhere.
- **i18n:** Simplified Chinese + English.

### Auto-update
- **In-app one-click update:** Detect → download → verify signature → install → "Update ready — restart now" button. No browser needed. 1h cache cooldown; manual check bypasses the cache.
- **Cross-platform system proxy:** Auto-detects OS proxy at startup (macOS `scutil`, Windows registry, Linux `HTTPS_PROXY`) so all network requests route through your Clash / V2Ray without hard-coded ports.

### Packaging
- **macOS:** `.dmg`, Developer ID-signed and Apple-notarized — no Gatekeeper warning.
- **Windows:** NSIS `-setup.exe` installer.
- **Linux:** `.AppImage`.

## Download

All installers are built and attached automatically by CI. Pick your platform from the **Assets** below (or the [latest release](https://github.com/lzc0088/token-usage/releases/latest)):

- **macOS (Apple Silicon)** — `.dmg`, signed + notarized
- **Windows 10 / 11** — `-setup.exe`
- **Linux x64** — `.AppImage`

> `.app.tar.gz` / `.sig` / `.AppImage.tar.gz` files are updater packages — ignore them unless you're debugging the in-app updater.

<details>
<summary><strong>Install & first-run notes</strong></summary>

### macOS
Open the `.dmg`, drag **Token Usage** to **Applications**. Signed + notarized, no Gatekeeper block. If macOS asks on first launch: right-click → **Open**.

### Windows
Run the `-setup.exe` installer. Installs to `%LOCALAPPDATA%`, adds a Start Menu entry.

### Linux
```bash
chmod +x Token-Usage*.AppImage
./Token-Usage*.AppImage
```

### First run
The app launches into menu bar / tray mode — **no account or signup needed**. To check vendor quotas, fill in the corresponding credential under **Settings → Quotas** (API Key / Cookie / OAuth).

</details>

---

# 中文

## 新增功能

### 采集与会话
- **18+ AI 工具实时追踪：** Claude Code、Codex、ZCode、OpenCode、Qoder、Trae、Cursor、Copilot、Kimi、GLM、DeepSeek、Grok、Qwen、MiMo 等——每轮对话后 UI 在数秒内刷新。
- **单会话明细：** 打开一个 session，逐条提问拆分 token、模型与用到的工具，可展开每次回复的输入/输出/缓存命中/缓存写入分布。
- **缓存命中统计：** 点任一工具或模型，展开查看缓存命中与未命中的输入 token 分类及命中率百分比。
- **项目 / 工作区分组：** 从工具记录中的 `cwd` 反推项目名，按工作区或仓库自动聚合。
- **使用趋势：** 近 7 天 / 当月折线图，日均与峰值统计。
- **实时速率：** 模型忙时算 output tokens/s（速度）或总 tokens/min（消耗速率），可切换。
- **成本与币别：** USD / CNY 切换显示，汇率每日自动更新，也可在设置中手动覆写。

### 额度
- **17 家厂商额度：** Claude、Codex、Cursor、OpenRouter、GLM、Kimi、Volcengine、Ollama、Minimax、DeepSeek、iFlytek、Stepfun、Qoder、Copilot、Grok、MiMo——每家的 session / 每周 / 账单 / credits 消耗窗口独立展示，到期时间、余额、百分比一清二楚。
- **多账号 / 多凭证：** 同一厂商可配多个账号；OAuth（Copilot / Codex）、API Key、Cookie 三类凭证并存，AES-256-GCM + 机器派生密钥加密落盘。

### 采集
- **三种采集模式：** 实时（file-watch 推送）/ 智能（10min 间隔 + 活动感知）/ 定时间隔。
- **会话保留与归档：** 工具卸载或旧 session 被清后，本地已统计的每日数据不丢失；归档会话可在设置中查看与清空。

### 界面
- **菜单栏 & 系统托盘：** macOS 菜单栏、Windows / Linux 系统托盘。自定义显示：今日 / 累计 Tokens 或成本、仅图标。右键菜单快速操作。
- **Popover 弹窗：** 点托盘图标弹出 458px 面板，包含时段切换、总览、工具/模型拆解、额度、趋势、会话、项目等全部模块。
- **设置（5 分区独立窗口）：** 常规（语言/主题/更新）/ 预览界面 / 窗口（弹窗/固定/置顶模式 + 全局热键）/ 采集追踪（模式 + 工具列表 + 归档）/ 账号额度（厂商 + 凭证 + 刷新间隔）。
- **外观：** 暗 / 亮 / 跟随系统；可调玻璃透明度与模糊度；完全透明窗口模式。
- **全局热键：** 可录制快捷键，从任意位置唤出 popover。
- **i18n：** 简体中文 + 英文。

### 自动更新
- **应用内一键更新：** 检测 → 下载 → 验签 → 安装 → "更新已就绪，重启后生效"——全程不出浏览器。1 小时缓存冷却，手动检查可绕过。
- **跨平台系统代理：** 启动时自动读取系统代理（macOS `scutil`、Windows 注册表、Linux `HTTPS_PROXY`），更新/汇率/额度请求自动走 Clash / V2Ray，无需写死端口。

### 安装包
- **macOS：** `.dmg`，Developer ID 签名 + Apple 公证——无 Gatekeeper 拦截。
- **Windows：** NSIS `-setup.exe` 安装包。
- **Linux：** `.AppImage`。

## 下载

安装包由 CI 针对此 tag 自动构建并附在下方 **Assets** 中（或见 [最新 release](https://github.com/lzc0088/token-usage/releases/latest)），按平台选取：

- **macOS (Apple Silicon)** — `.dmg`，已签名 + 已公证
- **Windows 10 / 11** — `-setup.exe`
- **Linux x64** — `.AppImage`

> `.app.tar.gz` / `.sig` / `.AppImage.tar.gz` 是应用内更新用的更新包，非调试用途可忽略。

<details>
<summary><strong>安装与首次启动说明</strong></summary>

### macOS
打开 `.dmg`，把 **Token Usage** 拖进 **Applications**。已签名公证，无 Gatekeeper 拦截。若首次被询问：右键 → **打开**。

### Windows
运行 `-setup.exe` 安装包。装到 `%LOCALAPPDATA%`，加入开始菜单。

### Linux
```bash
chmod +x Token-Usage*.AppImage
./Token-Usage*.AppImage
```

### 首次启动
应用启动后进入菜单栏 / 托盘模式——**无需账号或注册**。要查厂商额度，在「设置 → 账号额度」中填对应凭证（API Key / Cookie / OAuth）。

</details>
