# macOS 原生桌面小组件（WidgetKit 扩展）

系统桌面小组件库（桌面右键 → 编辑小组件）中的 **Token Usage** 分组，含 3 个小组件：

| 小组件 | 尺寸 | 内容 |
|---|---|---|
| 今日总览 | 小 / 中 | 今日 token（琥珀渐变大数字）+ 费用；中号附实时速率与近 7 日趋势线 |
| 额度 | 小 / 中 | 最紧张的额度窗口进度（小号 1 条 / 中号 4 条，≥80% 转琥珀警示色） |
| 用量细分 | 中 | 今日 token Top 4，添加时可选择**按工具 / 按模型** |

## 架构（Metrik 的免 App Group 方案）

```
Rust 主进程 ──stdin──▶ publisher helper（嵌 widget bundle id）
                            │ UserDefaults.standard
                            ▼
              widget 扩展容器的 preferences plist ◀── widget 直接读
Rust 主进程 ──▶ reloader helper（嵌宿主 bundle id）──▶ WidgetCenter.reloadTimelines
```

- **不用 App Group**：无 Team ID 的 ad-hoc 签名下系统静默忽略 app group
  （macOS 15+ 手拼 `~/Library/Group Containers/` 也会被 TCC 拒绝）。publisher
  helper 嵌 **widget 扩展的 bundle id**（`com.tokenusage.desktop.widget`），其
  `UserDefaults.standard` 写入直接落进 widget 沙盒容器的同一份 plist
- **主动刷新**：reloader helper 嵌**宿主 app 的 bundle id**，直接调
  `WidgetCenter.reloadTimelines(ofKind:)`（数据更新即刷新，不再等 15 分钟周期）
- **全 ad-hoc 签名即可上小组件库**（无需 Developer ID / 钥匙串授权；
  发布时 CI 的 Developer ID 签名同样兼容）
- appex 的 `CFBundleVersion` 必须与主 app 一致 —— WidgetKit 把扩展的 bundle
  stub 归档进 timeline 并按 LaunchServices 校验，版本不一致会让升级后的
  缓存 stub 失配

## 构建

- `macos-widget/build.sh`：swiftc 直编（无 Xcode 工程）appex + 2 个 helper，
  `-application-extension -Xlinker -e -Xlinker _NSExtensionMain` 入口、
  `TAURI_ENV_ARCH=universal` 时双架构
- 正式构建：`tauri.conf.json` 的 `beforeBundleCommand` 自动编译，
  `bundle.resources` 把 appex 映射到 `PlugIns/`、helpers 到 `Helpers/`
- 本地快速迭代：`npm run tauri build` 后 `npm run widget:install`（对已有
  bundle 重新嵌入）

## 验证清单

1. `.app/Contents/PlugIns/TokenUsageWidget.appex` 存在且 adhoc 签名
2. 启动 app → `log stream --predicate 'subsystem == "com.tokenusage.desktop.widget"'`
   或直接 `defaults read` widget 容器 plist 出现 `widgetSnapshotJSON`
3. 桌面右键 → 编辑小组件 → 搜索 Token Usage
