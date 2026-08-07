# English

## What's changed

### Added
- **In-app auto-update:** Detect a new version → download → verify signature → install, all within the app. The app no longer vanishes silently after installing — it shows "Update ready" and a **Restart now** button you click when you're ready. (1h cache cooldown; manual check bypasses it.)
- **Cross-platform system proxy:** The app now reads your OS system proxy (macOS `scutil` / Windows registry / Linux `HTTPS_PROXY`) at startup, so the updater, exchange-rate, and quota requests automatically route through your Clash / V2Ray. No hard-coded port.

### Improved
- **macOS signing & notarization:** The `.dmg` is now Developer ID-signed and Apple-notarized. Opening it no longer shows "damaged" / "unidentified developer" warnings.
- **Docs:** README is now bilingual (English + Simplified Chinese) with a full Showcase.

### Fixed
- **Update check vs install mismatch:** `check_update` (reads the GitHub Release API) and `install_update` (reads `latest.json`) now agree on the version, so "Update now" no longer reports "already latest".

## Download

Installers are built and attached automatically by CI for this tag. Pick the one for your platform from the **Assets** list below (or the [latest release](https://github.com/lzc0088/token-usage/releases/latest)):

- **macOS (Apple Silicon)** — `.dmg`, signed + notarized
- **Windows 10 / 11** — `-setup.exe` (NSIS installer)
- **Linux x64** — `.AppImage`

> The `.app.tar.gz` / `-setup.exe.sig` / `.AppImage.tar.gz` files are updater packages — ignore them unless you're debugging the in-app updater.

<details>
<summary><strong>First launch & install notes</strong></summary>

### macOS
Open the `.dmg`, drag **Token Usage** to **Applications**. It's signed and notarized, so no Gatekeeper warning. On first run, if macOS still asks: right-click → **Open**.

### Windows
Run the `-setup.exe` installer. The app installs to `%LOCALAPPDATA%` and adds a Start Menu entry.

### Linux
Give the AppImage execute permission, then run:

```bash
chmod +x Token-Usage*.AppImage
./Token-Usage*.AppImage
```

### First run
The app launches into menu-bar / tray mode — **no account needed**. To check vendor quotas, fill in the corresponding credential under **Settings → Quotas** (API Key / Cookie / OAuth).

</details>

---

# 中文

## 更新内容

### 新增
- **应用内自动更新：** 检测到新版本 → 下载 → 验签 → 安装，全程在应用内完成。安装后应用**不再无声关闭**，而是显示"更新已就绪"+「立即重启」按钮，你点一下才重启。（1 小时缓存冷却，手动检查可绕过。）
- **跨平台系统代理：** 应用启动时自动读取系统代理（macOS `scutil` / Windows 注册表 / Linux `HTTPS_PROXY`），更新下载、汇率、额度请求会自动走你的 Clash / V2Ray，不再写死端口。

### 改进
- **macOS 签名与公证：** `.dmg` 已用 Developer ID 签名并通过 Apple 公证，打开不再提示"已损坏 / 无法验证开发者"。
- **文档：** README 改为中英双语（英文 + 简体中文），含完整界面截图 Showcase。

### 修复
- **检查更新与安装版本不一致：** `check_update`（读 GitHub Release API）与 `install_update`（读 `latest.json`）现在版本口径一致，点"立即更新"不再误报"已是最新版本"。

## 下载

安装包由 CI 针对该 tag 自动构建并附在下方 **Assets** 里（或看 [最新 release](https://github.com/lzc0088/token-usage/releases/latest)），选对应平台：

- **macOS (Apple Silicon)** — `.dmg`，已签名 + 已公证
- **Windows 10 / 11** — `-setup.exe`（NSIS 安装包）
- **Linux x64** — `.AppImage`

> 其中的 `.app.tar.gz` / `-setup.exe.sig` / `.AppImage.tar.gz` 是应用内更新用的更新包，除非你在调试自动更新，否则忽略它们。

<details>
<summary><strong>首次启动与安装说明</strong></summary>

### macOS
打开 `.dmg`，把 **Token Usage** 拖进 **Applications**。已签名公证，无 Gatekeeper 拦截。若首次仍被询问：右键 → **打开**。

### Windows
运行 `-setup.exe` 安装包。应用装到 `%LOCALAPPDATA%`，并加入开始菜单。

### Linux
给 AppImage 执行权限后运行：

```bash
chmod +x Token-Usage*.AppImage
./Token-Usage*.AppImage
```

### 首次启动
应用启动后进入菜单栏 / 托盘模式——**不需要账号**。要查厂商额度，在「设置 → 账号额度」里填对应凭证（API Key / Cookie / OAuth）。

</details>
