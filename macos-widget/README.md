# macOS 原生桌面小组件（WidgetKit 扩展）

系统桌面小组件库（桌面右键 → 编辑小组件）中的 **Token Usage** 分组，含 3 个小组件：

| 小组件 | 尺寸 | 内容 |
|---|---|---|
| 今日总览 | 小 / 中 | 今日 token（琥珀渐变大数字）+ 费用；中号附实时速率与近 7 日趋势线 |
| 额度 | 小 / 中 | 最紧张的额度窗口进度（小号 1 条 / 中号 4 条，≥80% 转琥珀警示色） |
| 用量细分 | 中 | 今日 token Top 4，添加时可选择**按工具 / 按模型** |

## 架构

```
Rust 主进程 ──(today/quota 数据变化 60s 防抖 + 5min 兜底)──▶ App Group 快照 JSON
macOS 系统 ──(每 ~15min 周期刷新)──▶ TokenUsageWidget.appex（.app/Contents/PlugIns/）
```

- 快照：`src-tauri/src/ui/widget_snapshot.rs` 写 `widget_snapshot.json`（原子写）
- 路径解析走 `NSFileManager.containerURL(forSecurityApplicationGroupIdentifier:)`
  （macOS 15+ 手拼 `~/Library/Group Containers/` 会触发 TCC 拒绝/弹窗）
- App Group：`group.2F74TS79TL.tokenusage`（**TeamID 前缀**形式 —— Developer ID
  签名无需 provisioning profile；三处必须一致：`widget_snapshot.rs` 的
  `APP_GROUP`、`TokenUsageWidget.entitlements`、`src-tauri/Entitlements.plist`）
- 刷新：WidgetKit 系统预算（~15 分钟），WidgetCenter 无 ObjC 桥无法由 Rust 主动
  触发 —— 与 token-monitor 的最终形态一致

## 本地构建与手测

```bash
npm run tauri build          # 主 app（Rust 常量变化时必须重跑）
npm run widget:install       # 编译+签 .appex，塞入 .app 并带 entitlements 重签
open "src-tauri/target/release/bundle/macos/Token Usage.app"
```

启动 ~30s 后快照落盘，然后：桌面右键 → 编辑小组件 → 搜索 Token Usage。

**注意**：本机钥匙串需有 Developer ID 证书（signingIdentity 从 tauri.conf 读取）；
adhoc 签名的 appex 无法通过 App Group 沙盒访问（小组件库也不会收录）。

## 正式发布

TeamID 前缀 App Group + Developer ID 签名即可随版分发（当前配置）。
若未来要换成非 TeamID 前缀的 group（`group.xxx`），需按 token-monitor #689 的
方式申请 Developer ID provisioning profiles（主 app 与 appex 各一）并配 CI secrets。

## 关键文件

- `Snapshot.swift` / `Provider.swift` / `Views.swift` / `TokenUsageWidget.swift`
- `Info.plist`（`com.apple.widgetkit-extension`；bundle id 为主 app id + `.widget`）
- `build.sh`（swiftc 直编 .appex，无 Xcode 工程；`--universal` 产双架构）
