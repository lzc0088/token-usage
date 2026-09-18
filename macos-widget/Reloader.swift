import Foundation
import WidgetKit

/// Widget reload helper (spawned by the host Rust process after publishing a
/// fresh snapshot). This binary embeds the HOST app's bundle identity
/// (com.tokenusage.desktop) so WidgetCenter attributes the reload to Token
/// Usage, not an unaffiliated bare executable. WidgetCenter dispatches via
/// async XPC — running the RunLoop briefly ensures the request is actually
/// enqueued before the process exits.
@main
enum TokenUsageWidgetReloader {
    static func main() {
        WidgetCenter.shared.reloadTimelines(ofKind: "TokenUsageTodayOverview")
        WidgetCenter.shared.reloadTimelines(ofKind: "TokenUsageQuotas")
        WidgetCenter.shared.reloadTimelines(ofKind: "TokenUsageBreakdown")
        RunLoop.current.run(until: Date().addingTimeInterval(0.5))
    }
}
