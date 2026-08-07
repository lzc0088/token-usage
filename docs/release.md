# 发布新版本

## 发布链路总览

```
开发者本地:
  ① 改版本号 (package.json / Cargo.toml / tauri.conf.json)
  ② 更新 RELEASE_NOTES.md (中英双语)
  ③ 提交
  ④ git tag vX.Y.Z && git push origin vX.Y.Z

GitHub Actions (release.yml):
  监听 push tags: "v*"
  ├─ macOS (aarch64)  ──▶ 签名 + 公证 → 打包 .dmg + .app.tar.gz
  ├─ Ubuntu (x86_64)  ──▶ 打包 .AppImage
  └─ Windows (x86_64) ──▶ 打包 -setup.exe (NSIS)
      │
      └── tauri-action@v1 自动:
           ├─ 创建 GitHub Release "Token Usage vX.Y.Z"
           ├─ 上传安装包 + .sig 签名文件
           ├─ 生成 + 上传 latest.json (应用内更新用)
           └─ Release body = scripts/RELEASE_NOTES.md 内容

终端用户:
  应用内 "检查更新" → 读 GitHub Release API → 检出 latest.json → 下载 → 安装
```

## 操作步骤

### ① 同步版本号（三处必须一致，且与 tag 严格匹配）

```bash
# 编辑以下 3 个文件，把 version 改成同一个值（例如 1.0.4）：

# 1. package.json
#    "version": "1.0.4"

# 2. src-tauri/Cargo.toml
#    [package]
#    version = "1.0.4"

# 3. src-tauri/tauri.conf.json
#    "version": "1.0.4"
```

> **必须同步**：tag 版本 → 三个 manifest 的 version —— 任何一处不一致都会导致：
> - 应用内「检查更新」—— 用户看到的版本与实际不一致
> - `install_update` —— 读 `latest.json` 里的 version 与 `get_app_version()` 返回的 source version 比，不一致则误报"已是最新"
>
> 格式：**纯数字**，不加 `v` 前缀。例如：`1.0.3`、`1.0.4`（不是 `v1.0.3`）。

### ② 更新 Release 说明

编辑 `scripts/RELEASE_NOTES.md`，按中英双语格式填写本次版本的更新内容。

格式（参考 [token-monitor v0.42.0](https://github.com/Javis603/token-monitor/releases/tag/v0.42.0)）：

```markdown
# English

## What's changed

### Added
- **Feature:** description

### Improved
- **Improvement:** description

### Fixed
- **Fix:** description

## Download
...（平台列表，不写死 URL——CI 自动附安装包）

---

# 中文

## 更新内容

### 新增
- **功能：** 描述

### 改进
- **改进：** 描述

### 修复
- **修复：** 描述

## 下载
...
```

**约定**：
- EN 段在前，中文段在后，用 `---` 分隔
- `### Added/新增`、`### Improved/改进`、`### Fixed/修复` 三个分类，无变更的分类可省略不写
- 每条用 `- **标签:** 说明` 格式（标签加粗，冒号后跟一句完整描述）
- 每条可附 `(#123)` 引用对应的 issue/PR
- «Download / 下载» 列出平台与文件后缀模式即可，不写死完整 URL（tag 不同 URL 不同）

### ③ 提交

```bash
git add -A
git commit -m "chore: 发布 v1.0.4"
git push origin master
```

### ④ 打 tag 并推送（触发 CI 构建 + 发布）

```bash
git tag v1.0.4
git push origin v1.0.4
```

> **注意**：
> - 只推分支**不触发** release.yml。必须单独 `git push origin v1.0.4`。
> - tag 格式必须是 `v*`（release.yml 监听 `v*` 通配），不要省略 `v` 前缀。
> - 想撤销 tag：`git tag -d v1.0.4 && git push origin :refs/tags/v1.0.4`。

## 检查清单

发版前逐项确认：

- [ ] `package.json` / `Cargo.toml` / `tauri.conf.json` version 三处**完全一致**
- [ ] tag 名 `vX.Y.Z` 与三处 version **去掉 `v` 后一致**（`v1.0.4` ↔ `1.0.4`）
- [ ] `scripts/RELEASE_NOTES.md` 已按本次实际改动更新（EN + 中文双段）
- [ ] 所有改动已提交（`git status` 干净）
- [ ] 本地测试通过：`npm run check` + `cargo clippy -- -D warnings` + `cargo test --lib`
- [ ] Apple Developer 证书仍在有效期内（macOS 签名依赖）← 每年 4–6 月检查
- [ ] SignPath GitHub Secrets 已配置（Windows 签名依赖）← 首次配置后除非重新签发 token，否则不需再动

## Windows 代码签名（SignPath Foundation）

Windows 安装包默认 **未签名**，双击会触发 SmartScreen "已阻止可能不安全的应用"。消除该警告需要对接 [SignPath Foundation](https://signpath.io/) 的免费开源代码签名服务。

### 注册步骤（仅首次，一次性）

1. 打开 [signpath.io](https://signpath.io/)，用 GitHub 账号注册/登录
2. Create Organization → 名称自定（如 `token-usage`）
3. Create Project → `Project slug` 填 `token-usage`（记下来，后面用作 CI secret）
   - Repository URL: `https://github.com/lzc0088/token-usage`
   - 启用 **Trusted Build System** + **Origin Verification**
4. 在 Project 下创建 **Signing Policy**：
   - `Signing Policy Slug` 填 `release-signing`
   - Certificate: 选择 SignPath Foundation 提供的免费证书
5. 创建 **Artifact Configuration**：
   - Root element: `<zip-file>`（upload-artifact@v4 会将文件打包为 ZIP）
   - Contents: `<pe-file>`（即 Windows .exe 安装包）
6. 创建 **API Token**（Settings → API Tokens）：
   - 权限：Submit signing requests
   - 复制 token → 存入 GitHub Secrets：`SIGNPATH_API_TOKEN`

### GitHub Secrets 配置

在仓库 Settings → Secrets and variables → Actions，新增 3 个 secret：

| Secret | 值 | 说明 |
|--------|-----|------|
| `SIGNPATH_API_TOKEN` | 第 6 步生成的 API token | 认证凭证 |
| `SIGNPATH_ORGANIZATION_ID` | SignPath 组织页面 URL 中的 UUID | 形如 `2e13633d-…` |
| `SIGNPATH_PROJECT_SLUG` | 第 3 步填的 project slug | 如 `token-usage` |

### CI 行为

配置完成后，每次 `git push v*` 触发 release.yml：
1. 三平台构建（macOS / Linux / Windows）并行跑
2. Windows 安装包 → 上传到 GitHub Release（**未签名**）
3. `sign-windows` job 启动：下载未签名 .exe → 提交 SignPath → 获签 → 替换 Release 中的旧文件

从用户角度看：Release 页面的 Windows .exe 在发布后约 5–10 分钟变为已签名版本（SignPath 处理时间）。

如果希望先发一个测试版而不是正式版，tag 中包含 `-` 后缀即自动标为 "Pre-release"：

```bash
git tag v1.0.4-beta
git push origin v1.0.4-beta
```

GitHub Release 会显示 "Pre-release" 标签，应用内更新检查**默认不检出 prerelease**。

正式版取消 `-beta`：

```bash
git tag v1.0.4
git push origin v1.0.4
```

## 故障排查

### release 未创建
- 检查 tag 命名是否以 `v` 开头（`1.0.4` 不触发 release.yml，必须是 `v1.0.4`）
- 检查 GitHub Actions 页面 → release workflow 是否有报错
- 如果 CI 部分平台失败（如 Windows 超时），release 可能不完整（asset 缺项）

### 应用内更新报"已是最新"
- 确认三处 manifest version 与打出的 tag 完全匹配（最常见原因）
- 确认 `latest.json` 里的 version 字段与安装包文件名的版本一致
- 清缓存：`sqlite3 ~/Library/Application\ Support/token-usage/token-usage.db "DELETE FROM app_config WHERE key LIKE 'update_%';"`
- 在设置页点「检查更新」强制绕过缓存重拉

### 下载极慢
- 应用已自动读取系统代理（macOS `scutil` / Windows 注册表 / Linux env）。如果仍慢，检查代理是否开"系统代理"模式
- GitHub Release CDN 对中国大陆可能较慢，这是上游问题

## 版本号策略

约定：**`主.次.修订`** 三段式。

| 段 | 什么时候加 | 示例 |
|----|-----------|------|
| `主` (major) | 大型架构变更、不向后兼容 | `2.0.0` |
| `次` (minor) | 新功能、不影响向后兼容 | `1.1.0` |
| `修订` (patch) | bug 修复、性能优化、文档 | `1.0.4` |

当前版本：**1.0.3**。
