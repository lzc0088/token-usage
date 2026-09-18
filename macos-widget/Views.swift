import SwiftUI
import WidgetKit

// Widget views — dark-only palette inspired by token-monitor's design system:
// near-black surface (#090B0E) with subtle accent gradient, monospaced digits,
// restrained 7.5–10pt labels, thin capsule bars, and a smooth Catmull-Rom trend.

// ── design tokens ───────────────────────────────────────────────────────────

private enum Tokens {
    // Spacing
    static let smallGap: CGFloat = 5
    static let mediumGap: CGFloat = 10
    static let sectionPad: CGFloat = 12

    // Typography — metric (the one big number per module)
    static let metricSmall: CGFloat = 30      // systemSmall family
    static let metricMedium: CGFloat = 40     // systemMedium family
    static let metricMini: CGFloat = 14       // inline totals (trend header)

    // Labels
    static let sectionTitle: CGFloat = 10
    static let rowLabel: CGFloat = 11
    static let microLabel: CGFloat = 10
    static let detailLabel: CGFloat = 9

    // Dividers
    static let dividerOpacity: CGFloat = 0.14

    // Bars
    static let barHeight: CGFloat = 3
    static let barOpacity: CGFloat = 0.82

    // Trend line
    static let trendStroke: CGFloat = 1.4
    static let trendHeightSmall: CGFloat = 26
    static let trendHeightMedium: CGFloat = 34
}

// ── palette ─────────────────────────────────────────────────────────────────

private struct Palette {
    let scheme: ColorScheme

    /// Near-black surface: token-monitor's `#090B0E` (0.035, 0.043, 0.055).
    var surface: Color {
        Color(red: 0.035, green: 0.043, blue: 0.055)
    }

    /// Primary metric numbers: token-monitor's `#F3FBF7` (243,251,247).
    var number: Color { Color(red: 243/255, green: 251/255, blue: 247/255) }

    /// Secondary labels: token-monitor's `#A3ADBB` (163,173,187).
    var muted: Color { Color(red: 163/255, green: 173/255, blue: 187/255) }

    /// Trend/chart accent: token-monitor's `#73BDF5` (115,189,245).
    var chartBlue: Color { Color(red: 115/255, green: 189/255, blue: 245/255) }

    /// Warm amber for hero digits and highlights (token-usage brand).
    var amber: Color {
        Color(red: 0.91, green: 0.69, blue: 0.29)
    }

    /// Amber digit gradient — three-stop ramp (light → mid → deep).
    var digitGradient: LinearGradient {
        LinearGradient(
            colors: [
                Color(red: 0.976, green: 0.906, blue: 0.733),
                Color(red: 0.910, green: 0.690, blue: 0.294),
                Color(red: 0.804, green: 0.580, blue: 0.235),
            ],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
        )
    }

    /// Subtle container background gradient (token-monitor pattern).
    var containerGradient: LinearGradient {
        LinearGradient(
            colors: [
                .white.opacity(0.025),
                Color.accentColor.opacity(0.10),
            ],
            startPoint: .topTrailing,
            endPoint: .bottomLeading
        )
    }
}

// ── shared pieces ───────────────────────────────────────────────────────────

/// Background: near-black base + subtle accent gradient (token-monitor pattern).
private struct WidgetBackground: View {
    var body: some View {
        ZStack {
            Palette(scheme: .dark).surface
            Palette(scheme: .dark).containerGradient
        }
    }
}

/// The big amber digit + unit pair (hero at widget scale).
private struct GradientDigits: View {
    let value: String
    let unit: String
    let size: CGFloat

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 2) {
            Text(value)
                .font(.system(size: size, weight: .semibold, design: .serif))
                .foregroundStyle(palette.digitGradient)
                .lineLimit(1)
                .minimumScaleFactor(0.65)
                .monospacedDigit()
            if !unit.isEmpty {
                Text(unit)
                    .font(.system(size: size * 0.40, weight: .semibold))
                    .foregroundStyle(palette.muted)
            }
        }
    }

    private let palette = Palette(scheme: .dark)
}

/// Horizontal progress capsule (quota/tool/model bar) — token-monitor style:
/// 3pt-high, vendor color at 0.82 opacity, rounded ends.
private struct CapsuleBar: View {
    let pct: Double
    var palette: Palette

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(Color.primary.opacity(0.10))
                Capsule()
                    .fill(palette.amber.opacity(Tokens.barOpacity))
                    .frame(width: max(3, geo.size.width * min(pct, 100) / 100))
            }
        }
        .frame(height: Tokens.barHeight)
    }
}

/// 7-point trend line using Catmull-Rom-like cubic bezier (token-monitor's
/// `SmoothTrendShape`). Falls back to straight lines on points ≤ 2.
struct SmoothTrendline: View {
    let points: [Int]

    var body: some View {
        GeometryReader { geo in
            let w = geo.size.width
            let h = geo.size.height
            let data = points.map(Double.init)
            let maxV = max(1, data.max() ?? 1)

            if data.count > 2 {
                Self.catmullRomPath(w: w, h: h, data: data, maxV: maxV)
                    .stroke(palette.chartBlue, style: StrokeStyle(lineWidth: Tokens.trendStroke, lineCap: .round, lineJoin: .round))
            } else {
                Self.straightPath(w: w, h: h, data: data, maxV: maxV)
                    .stroke(palette.chartBlue, style: StrokeStyle(lineWidth: Tokens.trendStroke, lineCap: .round, lineJoin: .round))
            }
        }
    }

    /// Catmull-Rom → cubic bezier segments.
    private static func catmullRomPath(w: CGFloat, h: CGFloat, data: [Double], maxV: Double) -> Path {
        Path { p in
            func pt(_ i: Int) -> CGPoint {
                let x = w * CGFloat(i) / CGFloat(data.count - 1)
                let y = h - 2 - (h - 4) * CGFloat(data[i] / maxV)
                return CGPoint(x: x, y: y)
            }
            p.move(to: pt(0))
            for i in 0..<data.count - 1 {
                let p0 = pt(max(0, i - 1))
                let p1 = pt(i)
                let p2 = pt(i + 1)
                let p3 = pt(min(data.count - 1, i + 2))
                let cp1 = CGPoint(x: p1.x + (p2.x - p0.x) / 6, y: p1.y + (p2.y - p0.y) / 6)
                let cp2 = CGPoint(x: p2.x - (p3.x - p1.x) / 6, y: p2.y - (p3.y - p1.y) / 6)
                p.addCurve(to: p2, control1: cp1, control2: cp2)
            }
        }
    }

    /// Straight-line fallback for ≤ 2 points.
    private static func straightPath(w: CGFloat, h: CGFloat, data: [Double], maxV: Double) -> Path {
        Path { p in
            for (i, v) in data.enumerated() {
                let x = w * CGFloat(i) / CGFloat(max(1, data.count - 1))
                let y = h - 2 - (h - 4) * CGFloat(v / maxV)
                i == 0 ? p.move(to: CGPoint(x: x, y: y)) : p.addLine(to: CGPoint(x: x, y: y))
            }
        }
    }

    private let palette = Palette(scheme: .dark)
}

/// Thin divider matching token-monitor's 0.14 opacity rule.
private struct ThinDivider: View {
    var body: some View {
        Rectangle()
            .fill(palette.muted.opacity(Tokens.dividerOpacity))
            .frame(height: 0.5)
    }
    private let palette = Palette(scheme: .dark)
}

// ── widget bodies ────────────────────────────────────────────────────────────

/// 今日总览 — small: digits + cost; medium adds rate + smooth trendline.
struct TodayOverviewView: View {
    let entry: SnapshotEntry
    @Environment(\.widgetFamily) private var family

    var body: some View {
        content
            .containerBackground(for: .widget) { WidgetBackground() }
    }

    @ViewBuilder
    private var content: some View {
        if let snap = entry.snapshot {
            VStack(alignment: .leading, spacing: Tokens.smallGap) {
                Text("今日")
                    .font(.system(size: Tokens.sectionTitle, weight: .semibold))
                    .foregroundStyle(palette.muted)
                GradientDigits(
                    value: snap.tokenText.value,
                    unit: snap.tokenText.unit,
                    size: family == .systemSmall ? Tokens.metricSmall : Tokens.metricMedium
                )
                Text(snap.cnyText)
                    .font(.system(size: Tokens.rowLabel, weight: .semibold, design: .monospaced))
                    .foregroundStyle(palette.amber)
                if family != .systemSmall {
                    if let rate = snap.today.rateTokS {
                        Text(String(format: "%.0f tok/s", rate))
                            .font(.system(size: Tokens.microLabel, design: .monospaced))
                            .foregroundStyle(palette.muted)
                            .monospacedDigit()
                    }
                    if snap.spark.count >= 2 {
                        SmoothTrendline(points: snap.spark)
                            .frame(height: Tokens.trendHeightSmall)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(Tokens.sectionPad)
        } else {
            EmptySnapshotView()
        }
    }

    private let palette = Palette(scheme: .dark)
}

/// 额度 — tightest quota windows with capsule progress bars.
struct QuotasView: View {
    let entry: SnapshotEntry
    @Environment(\.widgetFamily) private var family

    private var rows: [Snapshot.QuotaRow] {
        guard let snap = entry.snapshot else { return [] }
        return Array(snap.quotas.sorted { $0.usedPct > $1.usedPct }
            .prefix(family == .systemSmall ? 1 : 4))
    }

    var body: some View {
        content
            .containerBackground(for: .widget) { WidgetBackground() }
    }

    @ViewBuilder
    private var content: some View {
        if rows.isEmpty {
            EmptySnapshotView()
        } else {
            VStack(alignment: .leading, spacing: family == .systemSmall ? Tokens.smallGap : Tokens.mediumGap) {
                if family != .systemSmall {
                    Text("额度")
                        .font(.system(size: Tokens.sectionTitle, weight: .semibold))
                        .foregroundStyle(palette.muted)
                }
                ForEach(rows, id: \.self) { row in
                    VStack(alignment: .leading, spacing: 2) {
                        HStack {
                            Text("\(SnapshotMeta.vendorLabel(row.vendor)) · \(row.label)")
                                .font(.system(size: Tokens.rowLabel, weight: .medium))
                                .foregroundStyle(palette.number)
                                .lineLimit(1)
                            Spacer()
                            Text("\(Int(row.usedPct.rounded()))%")
                                .font(.system(size: Tokens.rowLabel, weight: .bold, design: .monospaced))
                                .foregroundStyle(row.usedPct >= 80 ? palette.amber : palette.number)
                                .monospacedDigit()
                        }
                        CapsuleBar(pct: row.usedPct, palette: palette)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(Tokens.sectionPad)
        }
    }

    private let palette = Palette(scheme: .dark)
}

/// 工具用量 — today's top tools with capsule bars.
struct ToolUsageView: View {
    let entry: SnapshotEntry
    @Environment(\.widgetFamily) private var family

    private var rows: [Snapshot.BreakdownRow] {
        guard let snap = entry.snapshot else { return [] }
        return Array(snap.tools.prefix(family == .systemSmall ? 2 : 4))
    }

    var body: some View {
        content
            .containerBackground(for: .widget) { WidgetBackground() }
    }

    @ViewBuilder
    private var content: some View {
        if rows.isEmpty {
            EmptySnapshotView()
        } else {
            VStack(alignment: .leading, spacing: Tokens.smallGap) {
                Text("工具 · 今日")
                    .font(.system(size: Tokens.sectionTitle, weight: .semibold))
                    .foregroundStyle(palette.muted)
                ForEach(rows, id: \.key) { row in
                    CapsuleBarRow(row: row, label: SnapshotMeta.toolLabel(row.key), palette: palette)
                }
                Spacer(minLength: 0)
            }
            .padding(Tokens.sectionPad)
        }
    }

    private let palette = Palette(scheme: .dark)
}

/// 模型用量 — today's top models with capsule bars.
struct ModelUsageView: View {
    let entry: SnapshotEntry
    @Environment(\.widgetFamily) private var family

    private var rows: [Snapshot.BreakdownRow] {
        guard let snap = entry.snapshot else { return [] }
        return Array(snap.models.prefix(family == .systemSmall ? 2 : 4))
    }

    var body: some View {
        content
            .containerBackground(for: .widget) { WidgetBackground() }
    }

    @ViewBuilder
    private var content: some View {
        if rows.isEmpty {
            EmptySnapshotView()
        } else {
            VStack(alignment: .leading, spacing: Tokens.smallGap) {
                Text("模型 · 今日")
                    .font(.system(size: Tokens.sectionTitle, weight: .semibold))
                    .foregroundStyle(palette.muted)
                ForEach(rows, id: \.key) { row in
                    CapsuleBarRow(row: row, label: SnapshotMeta.toolLabel(row.key), palette: palette)
                }
                Spacer(minLength: 0)
            }
            .padding(Tokens.sectionPad)
        }
    }

    private let palette = Palette(scheme: .dark)
}

/// Shared capsule bar row (tool or model).
private struct CapsuleBarRow: View {
    let row: Snapshot.BreakdownRow
    let label: String
    var palette: Palette

    var body: some View {
        HStack(spacing: 8) {
            Text(label)
                .font(.system(size: Tokens.rowLabel, weight: .medium))
                .foregroundStyle(palette.number)
                .lineLimit(1)
            CapsuleBar(pct: row.pct, palette: palette)
            Text(String(format: "%.1f%%", row.pct))
                .font(.system(size: Tokens.microLabel, design: .monospaced))
                .foregroundStyle(palette.muted)
                .monospacedDigit()
                .frame(width: 42, alignment: .trailing)
        }
    }
}

/// 趋势统计 — 7-day smooth trend chart (medium: line + total; large: + daily detail).
struct TrendChartView: View {
    let entry: SnapshotEntry
    @Environment(\.widgetFamily) private var family

    private var points: [Snapshot.TrendPoint] {
        entry.snapshot?.trendPoints ?? []
    }

    private var totalTokens: Int { points.reduce(0) { $0 + $1.tokens } }

    var body: some View {
        content
            .containerBackground(for: .widget) { WidgetBackground() }
    }

    @ViewBuilder
    private var content: some View {
        if points.isEmpty {
            EmptySnapshotView()
        } else {
            VStack(alignment: .leading, spacing: Tokens.smallGap) {
                // Header
                HStack {
                    Text("趋势 · 近 7 日")
                        .font(.system(size: Tokens.sectionTitle, weight: .semibold))
                        .foregroundStyle(palette.muted)
                    Spacer()
                    GradientDigits(
                        value: compactTokens(totalTokens).value,
                        unit: compactTokens(totalTokens).unit,
                        size: Tokens.metricMini
                    )
                }

                // Smooth trend line
                SmoothTrendline(points: points.map(\.tokens))
                    .frame(height: family == .systemLarge ? Tokens.trendHeightMedium : Tokens.trendHeightSmall)

                // Large: daily detail rows
                if family == .systemLarge {
                    ThinDivider()
                    ForEach(points.suffix(5).reversed(), id: \.date) { pt in
                        HStack {
                            Text(String(pt.date.suffix(5)))
                                .font(.system(size: Tokens.detailLabel, design: .monospaced))
                                .foregroundStyle(palette.muted)
                                .monospacedDigit()
                            Spacer()
                            Text(compactTokens(pt.tokens).value + compactTokens(pt.tokens).unit)
                                .font(.system(size: Tokens.microLabel, weight: .medium, design: .monospaced))
                                .foregroundStyle(palette.number)
                                .monospacedDigit()
                            Text(String(format: "$%.1f", pt.costUsd))
                                .font(.system(size: Tokens.detailLabel, design: .monospaced))
                                .foregroundStyle(palette.muted)
                                .monospacedDigit()
                        }
                    }
                }

                Spacer(minLength: 0)
            }
            .padding(Tokens.sectionPad)
        }
    }

    private let palette = Palette(scheme: .dark)
}

/// Shown before the app has ever exported a snapshot.
struct EmptySnapshotView: View {
    var body: some View {
        VStack(spacing: 4) {
            Image(systemName: "chart.bar.doc.horizontal")
                .font(.title3)
                .foregroundStyle(palette.muted)
            Text("启动 Token Usage 后显示数据")
                .font(.system(size: Tokens.microLabel))
                .foregroundStyle(palette.muted)
        }
        .containerBackground(for: .widget) { WidgetBackground() }
    }

    private let palette = Palette(scheme: .dark)
}

/// Compact zh-style token count (same logic as Snapshot.tokenText but standalone).
private func compactTokens(_ n: Int) -> (value: String, unit: String) {
    let d = Double(n)
    if d >= 100_000_000 {
        return (String(format: "%.2f", d / 100_000_000), "亿")
    }
    if d >= 10_000 {
        return (String(format: "%.1f", d / 10_000), "万")
    }
    return (String(n), "")
}
