import WidgetKit
import SwiftUI

@main
struct TokenUsageWidgetBundle: WidgetBundle {
    var body: some Widget {
        TodayOverviewWidget()
        QuotaWidget()
        ToolUsageWidget()
        ModelUsageWidget()
        TrendChartWidget()
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

/// 工具用量 — today's top tools with bar rows.
struct ToolUsageWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "TokenUsageToolUsage", provider: SnapshotProvider()) { entry in
            ToolUsageView(entry: entry)
        }
        .configurationDisplayName("工具用量")
        .description("今日 token 用量 Top 4 工具排行")
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

/// 模型用量 — today's top models with bar rows.
struct ModelUsageWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "TokenUsageModelUsage", provider: SnapshotProvider()) { entry in
            ModelUsageView(entry: entry)
        }
        .configurationDisplayName("模型用量")
        .description("今日 token 用量 Top 4 模型排行")
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

/// 趋势统计 — 7-day trend chart.
struct TrendChartWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "TokenUsageTrendChart", provider: SnapshotProvider()) { entry in
            TrendChartView(entry: entry)
        }
        .configurationDisplayName("趋势统计")
        .description("近 7 日 token 用量趋势图")
        .supportedFamilies([.systemMedium, .systemLarge])
    }
}
