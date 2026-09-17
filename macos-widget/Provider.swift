import WidgetKit

/// Timeline entry carrying the latest snapshot (nil until the app has run
/// at least once with fresh data).
struct SnapshotEntry: TimelineEntry {
    let date: Date
    let snapshot: Snapshot?

    /// Demo payload for the gallery / placeholder frame.
    static func sample() -> SnapshotEntry {
        let snap = Snapshot(
            updatedAt: 0,
            today: .init(tokens: 79_072_426, costUsd: 17.2, rateTokS: 182.5),
            quotas: [
                .init(vendor: "kimi", plan: "Kimi For Coding", label: "weekly", usedPct: 82, resetsAt: nil),
                .init(vendor: "glm", plan: "GLM Coding Plan", label: "5h", usedPct: 34, resetsAt: nil),
            ],
            tools: [
                .init(key: "claude", tokens: 65_000_000, pct: 82, costUsd: 12.1),
                .init(key: "workbuddy", tokens: 14_072_426, pct: 18, costUsd: 5.1),
            ],
            models: [
                .init(key: "glm-5.3", tokens: 32_500_000, pct: 41, costUsd: 6.0),
                .init(key: "step-3.7-flash", tokens: 43_371_264, pct: 55, costUsd: 6.8),
            ],
            spark: [42, 61, 38, 75, 52, 90, 68]
        )
        return SnapshotEntry(date: .now, snapshot: snap)
    }
}

struct SnapshotProvider: TimelineProvider {
    func placeholder(in context: Context) -> SnapshotEntry {
        .sample()
    }

    func getSnapshot(in context: Context, completion: @escaping (SnapshotEntry) -> Void) {
        completion(SnapshotEntry(date: .now, snapshot: Snapshot.load() ?? SnapshotEntry.sample().snapshot))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<SnapshotEntry>) -> Void) {
        let entry = SnapshotEntry(date: .now, snapshot: Snapshot.load())
        // Refresh budget: WidgetCenter cannot be triggered from the host's
        // Rust process (no ObjC bridge), so the widget polls the snapshot on
        // a 15-minute cadence — the file is rewritten by the app on every
        // data change, so the next system-granted refresh is always fresh.
        let next = Calendar.current.date(byAdding: .minute, value: 15, to: .now) ?? .now.addingTimeInterval(900)
        completion(Timeline(entries: [entry], policy: .after(next)))
    }
}
