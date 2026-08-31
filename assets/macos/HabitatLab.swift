import AppKit
import WebKit

private let scenarios = [
    "habitat_story_v1",
    "ecology_smoke",
    "orb_drag_throw",
    "orb_offer_user_ignores",
    "orb_offer_user_plays",
    "orb_intercept_miss_retry",
    "orb_window_bounce",
    "orb_trapped_help",
    "pet_window_squeeze_escape",
    "den_store_restart_retrieve",
    "focus_mode_return_home",
    "morsel_accept",
    "morsel_refuse",
    "saliency_shared_attention",
    "camouflage_readability",
    "teach_figure_eight",
    "skill_transfer_new_region",
    "rhythm_echo",
]

private func label(_ text: String) -> NSTextField {
    let field = NSTextField(labelWithString: text)
    field.textColor = .secondaryLabelColor
    return field
}

final class HabitatLabDelegate: NSObject, NSApplicationDelegate {
    private var window: NSWindow!
    private let scenarioPicker = NSPopUpButton()
    private let seedField = NSTextField(string: "42")
    private let ticksField = NSTextField(string: "720")
    private let runButton = NSButton(title: "Run scenario", target: nil, action: nil)
    private let revealButton = NSButton(title: "Show reports", target: nil, action: nil)
    private let statusField = NSTextField(labelWithString: "Ready")
    private let reportView = WKWebView()
    private var running = false

    private lazy var reportsDirectory: URL = {
        let support = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first!
        return support
            .appendingPathComponent("Pet 2", isDirectory: true)
            .appendingPathComponent("Habitat Lab", isDirectory: true)
    }()

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        installApplicationMenu()
        buildWindow()
        loadLatestReportIfPresent()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    private func installApplicationMenu() {
        let mainMenu = NSMenu()
        let applicationItem = NSMenuItem()
        mainMenu.addItem(applicationItem)
        let applicationMenu = NSMenu()
        applicationMenu.addItem(
            withTitle: "Quit Habitat Lab",
            action: #selector(NSApplication.terminate(_:)),
            keyEquivalent: "q"
        )
        applicationItem.submenu = applicationMenu
        NSApp.mainMenu = mainMenu
    }

    private func buildWindow() {
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1080, height: 720),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Habitat Lab"
        window.minSize = NSSize(width: 820, height: 560)
        window.center()

        let content = NSView()
        content.translatesAutoresizingMaskIntoConstraints = false
        window.contentView = content

        scenarioPicker.addItems(withTitles: scenarios)
        scenarioPicker.selectItem(withTitle: "habitat_story_v1")
        scenarioPicker.setContentHuggingPriority(.defaultLow, for: .horizontal)
        seedField.alignment = .right
        seedField.maximumNumberOfLines = 1
        ticksField.alignment = .right
        ticksField.maximumNumberOfLines = 1

        runButton.target = self
        runButton.action = #selector(runScenario)
        runButton.keyEquivalent = "\r"
        revealButton.target = self
        revealButton.action = #selector(revealReports)

        let controls = NSStackView(views: [
            label("Scenario"), scenarioPicker,
            label("Seed"), seedField,
            label("Ticks"), ticksField,
            runButton, revealButton,
        ])
        controls.orientation = .horizontal
        controls.alignment = .centerY
        controls.spacing = 10
        controls.translatesAutoresizingMaskIntoConstraints = false

        statusField.font = .systemFont(ofSize: 12)
        statusField.textColor = .secondaryLabelColor
        statusField.translatesAutoresizingMaskIntoConstraints = false
        reportView.translatesAutoresizingMaskIntoConstraints = false

        content.addSubview(controls)
        content.addSubview(statusField)
        content.addSubview(reportView)
        NSLayoutConstraint.activate([
            controls.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 16),
            controls.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -16),
            controls.topAnchor.constraint(equalTo: content.topAnchor, constant: 14),
            statusField.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 18),
            statusField.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -18),
            statusField.topAnchor.constraint(equalTo: controls.bottomAnchor, constant: 8),
            reportView.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            reportView.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            reportView.topAnchor.constraint(equalTo: statusField.bottomAnchor, constant: 10),
            reportView.bottomAnchor.constraint(equalTo: content.bottomAnchor),
            scenarioPicker.widthAnchor.constraint(greaterThanOrEqualToConstant: 220),
            seedField.widthAnchor.constraint(equalToConstant: 62),
            ticksField.widthAnchor.constraint(equalToConstant: 72),
        ])
    }

    @objc private func runScenario() {
        guard !running else { return }
        guard let scenario = scenarioPicker.selectedItem?.title else { return }
        let seed = seedField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        let ticks = ticksField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard UInt64(seed) != nil, UInt64(ticks) != nil else {
            statusField.stringValue = "Seed and ticks must be whole non-negative numbers."
            NSSound.beep()
            return
        }
        guard let runner = Bundle.main.url(
            forResource: "HabitatLabRunner",
            withExtension: nil
        ) else {
            statusField.stringValue = "HabitatLabRunner is missing from the app bundle."
            return
        }

        running = true
        runButton.isEnabled = false
        statusField.stringValue = "Running \(scenario)…"
        let reports = reportsDirectory
        let report = reports.appendingPathComponent("latest-habitat.svg")
        let trace = reports.appendingPathComponent("latest-trace.json")
        let state = reports.appendingPathComponent("latest-ecology.json")
        let summary = reports.appendingPathComponent("latest-summary.json")

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            do {
                try FileManager.default.createDirectory(
                    at: reports,
                    withIntermediateDirectories: true
                )
                let output = Pipe()
                let task = Process()
                task.executableURL = runner
                task.arguments = [
                    "--scenario", scenario,
                    "--seed", seed,
                    "--ticks", ticks,
                    "--screenshot", report.path,
                    "--trace", trace.path,
                    "--save", state.path,
                ]
                task.standardOutput = output
                task.standardError = output
                try task.run()
                let data = output.fileHandleForReading.readDataToEndOfFile()
                task.waitUntilExit()
                try data.write(to: summary, options: .atomic)
                guard task.terminationStatus == 0 else {
                    let message = String(data: data, encoding: .utf8) ?? "Unknown runner error"
                    throw NSError(
                        domain: "HabitatLab",
                        code: Int(task.terminationStatus),
                        userInfo: [NSLocalizedDescriptionKey: message]
                    )
                }
                DispatchQueue.main.async {
                    self?.running = false
                    self?.runButton.isEnabled = true
                    self?.statusField.stringValue = "Completed \(scenario)."
                    self?.loadReport(report)
                }
            } catch {
                DispatchQueue.main.async {
                    self?.running = false
                    self?.runButton.isEnabled = true
                    self?.statusField.stringValue = "Failed: \(error.localizedDescription)"
                }
            }
        }
    }

    @objc private func revealReports() {
        try? FileManager.default.createDirectory(
            at: reportsDirectory,
            withIntermediateDirectories: true
        )
        NSWorkspace.shared.activateFileViewerSelecting([reportsDirectory])
    }

    private func loadLatestReportIfPresent() {
        let report = reportsDirectory.appendingPathComponent("latest-habitat.svg")
        if FileManager.default.fileExists(atPath: report.path) {
            loadReport(report)
        } else {
            runScenario()
        }
    }

    private func loadReport(_ report: URL) {
        reportView.loadFileURL(report, allowingReadAccessTo: reportsDirectory)
    }
}

let application = NSApplication.shared
let delegate = HabitatLabDelegate()
application.delegate = delegate
application.run()
