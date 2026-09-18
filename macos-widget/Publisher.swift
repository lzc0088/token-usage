import Foundation

/// Snapshot publisher helper (spawned by the host Rust process with the
/// JSON snapshot on stdin). This binary embeds the WIDGET extension's
/// bundle identity (com.tokenusage.desktop.widget, via its linked-in
/// Info.plist), so its UserDefaults.standard writes land in the widget
/// extension's sandbox container — the exact preferences plist the widget
/// reads. This is the App-Group-free sharing path for ad-hoc builds
/// (App Groups are silently ignored without a Team ID).
@main
enum TokenUsageWidgetPublisher {
    static func main() {
        let data = FileHandle.standardInput.readDataToEndOfFile()
        guard !data.isEmpty else {
            FileHandle.standardError.write(Data("widget snapshot input is empty\n".utf8))
            exit(3)
        }
        let sharedDefaults = UserDefaults.standard
        sharedDefaults.set(data, forKey: "widgetSnapshotJSON")
        sharedDefaults.synchronize()
        // Diagnostic: print where the snapshot landed (the widget container's
        // preferences plist path).
        let preferences = URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
            .appendingPathComponent("Library/Preferences", isDirectory: true)
            .appendingPathComponent("com.tokenusage.desktop.widget.plist", isDirectory: false)
        FileHandle.standardOutput.write(Data("\(preferences.path)\n".utf8))
    }
}
