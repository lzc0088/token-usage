import SwiftUI
import WidgetKit

// Widget views — a compact port of the app's design system: warm-black
// surface, amber gradient digits, borderless elevation. Colors are literals
// (the widget process can't read the app's CSS tokens) with a light-mode
// ramp via colorScheme branches.

// ── palette ──────────────────────────────────────────────────────────────

private struct WidgetPalette {
    let dark: Bool

    var surface: Color { dark ? Color(red: 0.06, green: 0.055, blue: 0.043) : Color(red: 0.96, green: 0.95, blue: 0.93) }
    var card: Color { dark ? Color(red: 0.11, green: 0.10, blue: 0.08) : Color(red: 0.91, green: 0.89, blue: 0.85) }
    var text: Color { dark ? Color(red: 0.95, green: 0.93, blue: 0.88) : Color(red: 0.16, green: 0.14, blue: 0.10) }
    var textDim: Color { dark ? Color(red: 0.64, green: 0.61, blue: 0.55) : Color(red: 0.42, green: 0.39, blue: 0.33) }
    var textFaint: Color { dark ? Color(red: 0.54, green: 0.52, blue: 0.44) : Color(red: 0.50, green: 0.47, blue: 0.40) }
    var amber: Color { dark ? Color(red: 0.91, green: 0.69, blue: 0.29) : Color(red: 0.72, green: 0.51, blue: 0.12) }

    /// The app-wide amber digit ramp (hero/period capsule/lead card).
    var digitGradient: LinearGradient {
        dark
            ? LinearGradient(colors: [
                Color(red: 0.976, green: 0.906, blue: 0.733),
                Color(red: 0.910, green: 0.690, blue: 0.294),
                Color(red: 0.804, green: 0.580, blue: 0.235),
            ], startPoint: .topLeading, endPoint: .bottomTrailing)
            : LinearGradient(colors: [
                Color(red: 0.710, green: 0.494, blue: 0.110),
                Color(red: 0.604, green: 0.416, blue: 0.071),
                Color(red: 0.490, green: 0.335, blue: 0.063),
            ], startPoint: .topLeading, endPoint: .bottomTrailing)
    }
}

// ── shared pieces ────────────────────────────────────────────────────────

/// The big amber digit + unit pair (hero at widget scale).
private struct GradientDigits: View {
    let value: String
    let unit: String
    let size: CGFloat

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 2) {
            Text(value)
                .font(.system(size: size, weight: .medium, design: .serif))
                .foregroundStyle(palette.digitGradient)
                .lineLimit(1)
                .minimumScaleFactor(0.6)
            if !unit.isEmpty {
                Text(unit)
                    .font(.system(size: size * 0.42, weight: .semibold))
                    .foregroundStyle(palette.textDim)
            }
        }
    }

    @Environment(\.colorScheme) private var scheme
    private var palette: WidgetPalette { WidgetPalette(dark: scheme == .dark) }
}

/// Horizontal progress capsule (quota bar).
private struct QuotaBar: View {
    let pct: Double
    var palette: WidgetPalette

    private var fill: Color {
        pct >= 80 ? palette.amber : (scheme == .dark
            ? Color(red: 0.95, green: 0.93, blue: 0.88).opacity(0.9)
            : Color(red: 0.24, green: 0.20, blue: 0.13).opacity(0.75))
    }

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(Color.primary.opacity(0.08))
                Capsule()
                    .fill(fill)
                    .frame(width: max(3, geo.size.width * min(pct, 100) / 100))
            }
        }
        .frame(height: 5)
    }

    @Environment(\.colorScheme) private var scheme
}

/// 7-point sparkline with the amber area gradient.
struct Sparkline: View {
    let points: [Int]

    var body: some View {
        GeometryReader { geo in
            let w = geo.size.width
            let h = geo.size.height
            let maxV = max(1, points.map(Double.init).max() ?? 1)
            let coords = points.indices.map { i in
                CGPoint(
                    x: w * CGFloat(i) / CGFloat(max(1, points.count - 1)),
                    y: h - 2 - (h - 4) * CGFloat(points[i]) / CGFloat(maxV)
                )
            }
            let line = Path { p in
                guard let first = coords.first else { return }
                p.move(to: first)
                for c in coords.dropFirst() { p.addLine(to: c) }
            }
            let area = line
                .strokedPath(StrokeStyle(lineWidth: 0.01))
                .offsetBy(dx: 0, dy: 0)
            ZStack {
                // area under the line
                Path { p in
                    p.addPath(area)
                    p.addLines([CGPoint(x: 0, y: h), CGPoint(x: w, y: h)])
                }
                .fill(LinearGradient(colors: [palette.amber.opacity(0.28), .clear], startPoint: .top, endPoint: .bottom))
                line
                    .stroke(palette.amber, style: StrokeStyle(lineWidth: 1.4, lineCap: .round, lineJoin: .round))
            }
        }
    }

    @Environment(\.colorScheme) private var scheme
    private var palette: WidgetPalette { WidgetPalette(dark: scheme == .dark) }
}

// ── widget bodies ────────────────────────────────────────────────────────

/// 今日总览 — small: digits + cost; medium adds rate + sparkline.
struct TodayOverviewView: View {
    let entry: SnapshotEntry
    @Environment(\.widgetFamily) private var family

    var body: some View {
        content
            .containerBackground(for: .widget) { palette.surface }
    }

    @ViewBuilder
    private var content: some View {
        if let snap = entry.snapshot {
            VStack(alignment: .leading, spacing: 6) {
                Text("今日")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(palette.textFaint)
                GradientDigits(
                    value: snap.tokenText.value,
                    unit: snap.tokenText.unit,
                    size: family == .systemSmall ? 30 : 40
                )
                Text(snap.cnyText)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(palette.amber)
                if family != .systemSmall {
                    if let rate = snap.today.rateTokS {
                        Text(String(format: "⚡ %.0f tok/s", rate))
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(palette.textFaint)
                    }
                    if snap.spark.count >= 2 {
                        Sparkline(points: snap.spark)
                            .frame(height: 26)
                    }
                }
                Spacer(minLength: 0)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .padding(12)
        } else {
            EmptySnapshotView()
        }
    }

    @Environment(\.colorScheme) private var scheme
    private var palette: WidgetPalette { WidgetPalette(dark: scheme == .dark) }
}

/// 额度 — tightest quota windows with progress capsules.
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
            .containerBackground(for: .widget) { palette.surface }
    }

    @ViewBuilder
    private var content: some View {
        if rows.isEmpty {
            EmptySnapshotView()
        } else {
            VStack(alignment: .leading, spacing: family == .systemSmall ? 8 : 6) {
                if family != .systemSmall {
                    Text("额度")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(palette.textFaint)
                }
                ForEach(rows, id: \.self) { row in
                    VStack(alignment: .leading, spacing: 3) {
                        HStack {
                            Text("\(SnapshotMeta.vendorLabel(row.vendor)) · \(row.label)")
                                .font(.system(size: 11, weight: .medium))
                                .foregroundStyle(palette.text)
                                .lineLimit(1)
                            Spacer()
                            Text("\(Int(row.usedPct.rounded()))%")
                                .font(.system(size: 11, weight: .bold, design: .monospaced))
                                .foregroundStyle(row.usedPct >= 80 ? palette.amber : palette.text)
                        }
                        QuotaBar(pct: row.usedPct, palette: palette)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(12)
        }
    }

    @Environment(\.colorScheme) private var scheme
    private var palette: WidgetPalette { WidgetPalette(dark: scheme == .dark) }
}

/// 用量细分 — top tools/models with bar rows (dimension via AppIntent).
struct BreakdownView: View {
    let entry: DimensionEntry
    let dimension: Dimension

    private var rows: [Snapshot.BreakdownRow] {
        guard let snap = entry.snapshot else { return [] }
        return dimension == .tools ? snap.tools : snap.models
    }

    var body: some View {
        content
            .containerBackground(for: .widget) { palette.surface }
    }

    @ViewBuilder
    private var content: some View {
        if rows.isEmpty {
            EmptySnapshotView()
        } else {
            VStack(alignment: .leading, spacing: 6) {
                Text(dimension == .tools ? "工具 · 今日" : "模型 · 今日")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(palette.textFaint)
                ForEach(rows, id: \.key) { row in
                    HStack(spacing: 8) {
                        Text(SnapshotMeta.toolLabel(row.key))
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(palette.text)
                            .lineLimit(1)
                        GeometryReader { geo in
                            ZStack(alignment: .leading) {
                                Capsule().fill(Color.primary.opacity(0.08))
                                Capsule()
                                    .fill(palette.amber.opacity(0.85))
                                    .frame(width: max(3, geo.size.width * row.pct / 100))
                            }
                        }
                        .frame(height: 5)
                        Text(String(format: "%.1f%%", row.pct))
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(palette.textDim)
                            .frame(width: 42, alignment: .trailing)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(12)
        }
    }

    @Environment(\.colorScheme) private var scheme
    private var palette: WidgetPalette { WidgetPalette(dark: scheme == .dark) }
}

/// Shown before the app has ever exported a snapshot.
struct EmptySnapshotView: View {
    var body: some View {
        VStack(spacing: 4) {
            Image(systemName: "chart.bar.doc.horizontal")
                .font(.title3)
            Text("启动 Token Usage 后显示数据")
                .font(.system(size: 10))
        }
        .foregroundStyle(palette.textFaint)
        .containerBackground(for: .widget) { palette.surface }
    }

    @Environment(\.colorScheme) private var scheme
    private var palette: WidgetPalette { WidgetPalette(dark: scheme == .dark) }
}
