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

Windows 安装包默认**未签名**，双击会触发 SmartScreen "已阻止可能不安全的应用"。
消除该警告需要对接 [SignPath Foundation](https://signpath.io/) 的免费开源代码签名服务。

> **前提：** 项目是公开的 GitHub 仓库、使用 OSI 许可证（MIT ✅）、至少有
> 一次 release（打 tag 后即可满足）、仓库 owner 开启 GitHub MFA。
> Token Monitor 等知名开源项目都在用这个方案。

### 总流程（2–3 周）

```
发邮件申请 OSS 订阅 → 填表格 → 审批（几天）
  → 在 SignPath 控制台创建项目 + Trusted Build System + Signing Policy + API Token
  → 配 GitHub Secrets → CI 用 test-signing 试跑
  → 通知 SignPath 申请 production 证书审核
  → 切到 release-signing → 发布 → SmartScreen 显示 "SignPath Foundation"
```

### 第〇步：安装 SignPath GitHub App

1. 打开 https://github.com/apps/signpath
2. 点 **Install** → 选 **Only select repositories** → 勾选 `lzc0088/token-usage`
3. 点 **Install** 授权

> 这是 SignPath 验证构建来源必需的。不装的话 Trusted Build System 检查会失败。

### 第一步：申请 OSS 订阅

1. 发邮件到 **support@signpath.io**，主题：**OSS subscription for token-usage**

   邮件正文模板：
   ```
   Hi,

   I'd like to apply for the SignPath Open Source Code Signing subscription.

   Project: token-usage
   Repository: https://github.com/lzc0088/token-usage
   License: MIT
   Description: Cross-platform menu bar app for tracking AI agent token usage
     (Tauri 2.x + Rust + Svelte 5). Windows users currently see SmartScreen
     warnings — we need code signing to resolve this.

   Thanks,
   Ze Chuan
   ```

2. SignPath 会回复你一份 **OSS Request Form**（Excel 表单），填写提交

   表单需要填的内容（提前准备）：
   | 字段 | 值 |
   |------|-----|
   | Project name | `token-usage` |
   | Repository URL | `https://github.com/lzc0088/token-usage` |
   | License | MIT |
   | Project owner | Ze Chuan |
   | Contact email | 你的邮箱 |
   | CI/CD system | GitHub Actions |
   | Artifact description | Windows NSIS installer (.exe), inside a ZIP |
   | Number of expected releases/year | 预估数（如 12） |

3. 提交表单后等待审批（通常 **2–5 个工作日**）

4. 审批通过后，SignPath 会发邮件通知你，同时在 https://app.signpath.io 创建好
   Organization 和 Project。你登录后就能看到。

### 第二步：登录 SignPath 控制台确认

1. 打开 https://app.signpath.io → **Sign in** → 用 GitHub 账号登录
2. 左侧应该能看到 SignPath 创建好的 Organization（名如 `token-usage`）
3. 点进 Organization → 点进 Project（`token-usage`）

### 第三步：配置 Trusted Build System

1. 在 Project 页面，左侧菜单 **Trusted Build Systems**
2. 点 **Add Trusted Build System** → 选 **GitHub Actions**
3. 填写：

   | 字段 | 值 |
   |------|-----|
   | **Name** | `GitHub Actions - master` |
   | **Repository** | `lzc0088/token-usage` |
   | **Branch** | `master` |

4. 点 **Save**

### 第四步：创建 Artifact Configuration

> 这一步用 XML 文件（比 UI 操作更可靠，且有明确的配置记录）。

在项目根创建 `.signpath/artifact-configuration.xml`：

```xml
<?xml version="1.0" encoding="utf-8"?>
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
    <!-- upload-artifact@v4 wraps files in a ZIP → SignPath receives a ZIP -->
    <zip-file>
        <!-- The NSIS installer inside the ZIP is a PE file (Portable Executable)
             that needs Authenticode signing -->
        <pe-file path="*.exe">
            <authenticode-sign />
        </pe-file>
    </zip-file>
</artifact-configuration>
```

然后在 SignPath 控制台：
1. 左侧菜单 **Artifact Configurations** → **Upload**
2. 上传刚才的 `artifact-configuration.xml`
3. Slug 填 `default`

> 提交并 push 这个 XML 文件到仓库——之后 CI 跑的时候 SignPath 会拿仓库里的
> 这个配置来验签。

### 第五步：创建 Signing Policies（两个）

#### 5a. `test-signing`（先建，用于 CI 试跑）

1. 左侧 **Signing Policies** → **Add Signing Policy**
2. 填写：

   | 字段 | 值 |
   |------|-----|
   | **Name** | `Test Signing` |
   | **Slug** | `test-signing` |
   | **Certificate** | 选 SignPath 提供的 **Test Certificate**（自签名，仅测试用） |
   | **Artifact Digest** | SHA256 |

3. **不勾选** Trusted Build System verification（test 阶段先跳过）
4. 点 **Create**

#### 5b. `release-signing`（后建，正式签名）

1. 再点 **Add Signing Policy**
2. 填写：

   | 字段 | 值 |
   |------|-----|
   | **Name** | `Release Signing` |
   | **Slug** | `release-signing` |
   | **Certificate** | 选 **SignPath Foundation**（审批通过后才有生产证书） |
   | **Artifact Digest** | SHA256 |

3. ✅ 勾选 **Enable Trusted Build System verification**
4. 勾选 **Enable Origin verification**
5. **Submitters** → 添加 Step 6 创建的 CI user
6. 点 **Create**

### 第六步：创建 CI User + API Token

1. 左侧 **Users** → **Add CI User**
2. Name 填 `GitHub Actions CI`
3. 点 **Create** → 页面会显示 **API Token 明文**
4. ⚠️ **立刻复制 token**——关掉页面后无法再查看
5. 回到 `release-signing` policy → **Submitters** → 把这个 CI User 加进去

### 第七步：填入 GitHub Secrets

在 GitHub 仓库 → Settings → Secrets and variables → Actions，添加 3 个：

| Secret | 值 | 来源 |
|--------|-----|------|
| `SIGNPATH_API_TOKEN` | 第 6 步的 token | CI User 创建页 |
| `SIGNPATH_ORGANIZATION_ID` | 组织 UUID | https://app.signpath.io 打开 Organization → 地址栏 `/Organization/` 后的 UUID |
| `SIGNPATH_PROJECT_SLUG` | `token-usage` | 固定值（SignPath 审批时创建的 slug） |

### 第八步：修改 CI 用 `test-signing` 试跑

临时把 release.yml 中 `sign-windows` job 的 `signing-policy-slug` 改为 `test-signing`，打一个测试 tag 触发 CI：

```bash
git tag v1.0.3-test-sign && git push origin v1.0.3-test-sign
```

到 Actions 页面看 `sign-windows` job 是否绿色。绿色 = 链路通。

### 第九步：申请生产证书

1. 发邮件给 support@signpath.io，主题：**Production certificate for token-usage**
2. 正文说明 test-signing 已跑通，请求审核并下发生产证书
3. SignPath 审核通过后，`release-signing` policy 的 Certificate 状态会从 PENDING 变为 **VALID**

### 第十步：切回 `release-signing` + 创建公开签名政策页

1. release.yml 的 `signing-policy-slug` 改回 `release-signing`
2. 在仓库里创建一个公开页面（通常是 `docs/code-signing.md` 或在 README 里加一段），说明：
   - 本项目通过 SignPath Foundation 提供免费代码签名
   - 验证签名：右键 exe → Properties → Digital Signatures → 应显示 "SignPath Foundation"
   - 链接到 https://signpath.org

### 第十一步：正式发布

打正式 tag → CI 自动签名 → 用户双击 no more SmartScreen。

### CI 行为回顾

配置完成后，每次 `git push v*` 触发 release.yml：
1. 三平台构建（macOS / Linux / Windows）并行跑
2. Windows 安装包 → 上传到 GitHub Release（**未签名**）
3. `sign-windows` job 启动：下载未签名 .exe → 提交 SignPath → 获签 → 替换 Release 中的旧文件

从用户视角：Release 页面的 Windows exe 在发布后约 5–10 分钟变为已签名版本，SmartScreen 不再警告，发布者显示 "SignPath Foundation"。

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
