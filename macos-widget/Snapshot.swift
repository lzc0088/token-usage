import Foundation

/// Snapshot of the main app's data, written by the Rust process into the
/// shared App Group container (`widget_snapshot.json`). Field names match
/// the Rust serde output (snake_case → camelCase via the decoder strategy).
struct Snapshot: Codable {
    struct TodayBlock: Codable {
        let tokens: Int
        let costUsd: Double
        let rateTokS: Double?
    }

    struct QuotaRow: Codable, Hashable {
        let vendor: String
        let plan: String?
        let label: String
        let usedPct: Double
        let resetsAt: String?
    }

    struct BreakdownRow: Codable {
        let key: String
        let tokens: Int
        let pct: Double
        let costUsd: Double
    }

    let updatedAt: Int64
    let today: TodayBlock
    let quotas: [QuotaRow]
    let tools: [BreakdownRow]
    let models: [BreakdownRow]
    let spark: [Int]

    // ── App Group loading ────────────────────────────────────────────────
    // Must match `APP_GROUP` in src-tauri/src/ui/widget_snapshot.rs.
    static let appGroup = "group.2F74TS79TL.tokenusage"

    static func load() -> Snapshot? {
        guard
            let container = FileManager.default.containerURL(
                forSecurityApplicationGroupIdentifier: appGroup
            )
        else { return nil }
        let url = container.appendingPathComponent("widget_snapshot.json")
        guard let data = try? Data(contentsOf: url) else { return nil }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try? decoder.decode(Snapshot.self, from: data)
    }

    // ── Formatting helpers (shared by all widget views) ─────────────────

    /// Compact zh-style token count: ≥亿 → x.xx亿, ≥万 → x.x万, else plain.
    var tokenText: (value: String, unit: String) {
        let n = Double(today.tokens)
        if n >= 100_000_000 {
            return (String(format: "%.2f", n / 100_000_000), "亿")
        }
        if n >= 10_000 {
            return (String(format: "%.1f", n / 10_000), "万")
        }
        return (String(Int(n)), "")
    }

    /// CNY cost at the ~7.2 reference rate (the app shows both; the widget
    /// keeps one line and CNY is the primary locale).
    var cnyText: String {
        String(format: "¥%.1f", today.costUsd * 7.2)
    }
}

/// Display names for tool/model keys (mirrors the app's meta tables; only
/// the frequent ones — anything else falls back to the raw key, title-cased).
enum SnapshotMeta {
    static func toolLabel(_ key: String) -> String {
        let map = [
            "claude": "Claude Code",
            "codex": "Codex",
            "opencode": "OpenCode",
            "workbuddy": "WorkBuddy",
            "zcode": "ZCode",
            "qoder": "Qoder",
            "cursor": "Cursor",
            "kimi": "Kimi",
            "droid": "Factory Droid",
        ]
        return map[key] ?? key.prefix(1).uppercased() + key.dropFirst()
    }

    static func vendorLabel(_ key: String) -> String {
        let map = [
            "glm": "GLM",
            "kimi": "Kimi",
            "minimax": "Minimax",
            "qoder": "Qoder",
            "volcengine": "火山引擎",
            "bailian": "百炼",
            "stepfun": "阶跃",
            "openrouter": "OpenRouter",
            "ollama": "Ollama",
            "deepseek": "DeepSeek",
            "workbuddy": "WorkBuddy",
        ]
        return map[key] ?? key.prefix(1).uppercased() + key.dropFirst()
    }
}
