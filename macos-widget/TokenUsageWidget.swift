import WidgetKit
import SwiftUI
import AppIntents

/// 工具/模型 dimension picker for the breakdown widget (macOS 14+).
enum Dimension: String, AppEnum {
    case tools
    case models

    static var typeDisplayRepresentation: TypeDisplayRepresentation {
        "维度"
    }

    static var caseDisplayRepresentations: [Dimension: DisplayRepresentation] {
        [
            .tools: "按工具",
            .models: "按模型",
        ]
    }
}

struct BreakdownIntent: WidgetConfigurationIntent {
    static var title: LocalizedStringResource { "用量细分" }
    static var description: IntentDescription { "今日 token 用量 Top 4" }

    @Parameter(title: "维度", default: .tools)
    var dimension: Dimension
}

@main
struct TokenUsageWidgetBundle: WidgetBundle {
    var body: some Widget {
        TodayOverviewWidget()
        QuotaWidget()
        BreakdownWidget()
    }
}

/// 今日总览 — small (digits + cost) / medium (+ rate + 7-day sparkline).
struct TodayOverviewWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "TokenUsageTodayOverview", provider: SnapshotProvider()) { entry in
            TodayOverviewView(entry: entry)
        }
        .configurationDisplayName("今日总览")
        .description("今日 token 用量与费用；中号附实时速率和近 7 日趋势")
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

/// 额度 — tightest quota windows (small: top 1 / medium: top 4).
struct QuotaWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "TokenUsageQuotas", provider: SnapshotProvider()) { entry in
            QuotasView(entry: entry)
        }
        .configurationDisplayName("额度")
        .description("最紧张的额度窗口进度；接近耗尽自动转为琥珀色")
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

/// 用量细分 — top tools or models (dimension switchable in the picker).
struct BreakdownWidget: Widget {
    var body: some WidgetConfiguration {
        AppIntentConfiguration(
            kind: "TokenUsageBreakdown",
            intent: BreakdownIntent.self,
            provider: BreakdownProvider()
        ) { entry in
            BreakdownView(entry: entry, dimension: entry.dimension)
        }
        .configurationDisplayName("用量细分")
        .description("今日 token 用量 Top 4，可在添加时选择按工具或按模型")
        .supportedFamilies([.systemMedium])
    }
}

/// Entry carrying the picked dimension for the AppIntent-configured widget.
/// (Same snapshot payload; the dimension only affects the view.)
struct DimensionEntry: TimelineEntry {
    let date: Date
    let snapshot: Snapshot?
    let dimension: Dimension
}

/// Provider variant that resolves the intent's dimension into the entry.
struct BreakdownProvider: AppIntentTimelineProvider {
    func placeholder(in context: Context) -> DimensionEntry {
        DimensionEntry(date: .now, snapshot: SnapshotEntry.sample().snapshot, dimension: .tools)
    }

    func snapshot(for configuration: BreakdownIntent, in context: Context) async -> DimensionEntry {
        DimensionEntry(date: .now, snapshot: Snapshot.load(), dimension: configuration.dimension)
    }

    func timeline(for configuration: BreakdownIntent, in context: Context) async -> Timeline<DimensionEntry> {
        let entry = DimensionEntry(date: .now, snapshot: Snapshot.load(), dimension: configuration.dimension)
        let next = Calendar.current.date(byAdding: .minute, value: 15, to: .now) ?? .now.addingTimeInterval(900)
        return Timeline(entries: [entry], policy: .after(next))
    }
}
