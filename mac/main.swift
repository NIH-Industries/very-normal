// Very Normal Compose: a small floating window to type in, measured by
// very-normal. Each message is pasted into the app you were using (the one in
// front when the window opened, or whichever you've clicked into since) and
// Return is pressed for you. The window doesn't take focus away from that app:
// it's a non-activating panel, like Spotlight's.
//
// All the counting happens in `very-normal compose --stdio`: this sends it
// each key as a line of JSON and draws what comes back. The text is never
// saved anywhere.

import AppKit
import ApplicationServices

// MARK: - very-normal

struct Segment: Decodable {
    let t: String
    let fixed: Bool
}

struct State: Decodable {
    let segments: [Segment]
    let wpm: Double?
    let corrected: Double?
    let nudge: String?
    let send: String?
}

/// `very-normal compose --stdio`, running for as long as the window is open.
final class Engine {
    private let process = Process()
    private let input = Pipe()
    private let output = Pipe()
    private var pending = Data()

    init(binary: String, arguments: [String], onState: @escaping (State) -> Void) throws {
        process.executableURL = URL(fileURLWithPath: binary)
        process.arguments = arguments + ["compose", "--stdio"]
        process.standardInput = input
        process.standardOutput = output
        output.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard let self, !data.isEmpty else { return }
            self.pending.append(data)
            while let newline = self.pending.firstIndex(of: 0x0A) {
                let line = self.pending[..<newline]
                self.pending.removeSubrange(...newline)
                if let state = try? JSONDecoder().decode(State.self, from: line) {
                    DispatchQueue.main.async { onState(state) }
                }
            }
        }
        try process.run()
    }

    var running: Bool { process.isRunning }

    func send(_ fields: [String: String]) {
        guard process.isRunning,
              var data = try? JSONSerialization.data(withJSONObject: fields) else { return }
        data.append(0x0A)
        try? input.fileHandleForWriting.write(contentsOf: data)
    }

    /// Closes stdin, so very-normal saves what's left, and waits for it.
    func finish() {
        try? input.fileHandleForWriting.close()
        let deadline = Date().addingTimeInterval(5)
        while process.isRunning && Date() < deadline {
            Thread.sleep(forTimeInterval: 0.02)
        }
    }

    /// Where very-normal is: $VERY_NORMAL_BIN, the usual install places, then
    /// your login shell's PATH (an app opened from Finder doesn't get it).
    static func locate() -> String? {
        let env = ProcessInfo.processInfo.environment
        if let bin = env["VERY_NORMAL_BIN"], !bin.isEmpty { return bin }
        let home = NSHomeDirectory()
        let places = ["\(home)/.local/bin", "\(home)/.cargo/bin", "/opt/homebrew/bin", "/usr/local/bin"]
        for dir in places where FileManager.default.isExecutableFile(atPath: "\(dir)/very-normal") {
            return "\(dir)/very-normal"
        }
        let shell = env["SHELL"] ?? "/bin/zsh"
        let which = Process()
        which.executableURL = URL(fileURLWithPath: shell)
        which.arguments = ["-lc", "command -v very-normal"]
        let out = Pipe()
        which.standardOutput = out
        which.standardError = FileHandle.nullDevice
        guard (try? which.run()) != nil else { return nil }
        which.waitUntilExit()
        let path = String(decoding: out.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return path.isEmpty ? nil : path
    }
}

// MARK: - Sending to the other app

enum Deliver {
    static let v: CGKeyCode = 9
    static let returnKey: CGKeyCode = 36

    /// Whether we may press keys in other apps, asking the first time.
    static func allowed(prompt: Bool) -> Bool {
        if CGPreflightPostEventAccess() { return true }
        return prompt ? CGRequestPostEventAccess() : false
    }

    static func press(_ key: CGKeyCode, flags: CGEventFlags = [], to pid: pid_t) {
        let source = CGEventSource(stateID: .hidSystemState)
        for down in [true, false] {
            let event = CGEvent(keyboardEventSource: source, virtualKey: key, keyDown: down)
            event?.flags = flags
            event?.postToPid(pid)
        }
    }

    /// Pastes `text` into `app` and, unless told not to, presses Return.
    /// The clipboard is put back afterwards.
    static func paste(_ text: String, into app: NSRunningApplication, pressReturn: Bool) {
        let board = NSPasteboard.general
        let saved: [NSPasteboardItem] = (board.pasteboardItems ?? []).map { item in
            let copy = NSPasteboardItem()
            for type in item.types {
                if let data = item.data(forType: type) { copy.setData(data, forType: type) }
            }
            return copy
        }
        board.clearContents()
        board.setString(text, forType: .string)
        let ours = board.changeCount
        let pid = app.processIdentifier
        press(v, flags: .maskCommand, to: pid)
        if pressReturn {
            // give the paste a moment to land before Return goes after it
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { press(returnKey, to: pid) }
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) {
            // only if nothing else has used the clipboard since
            guard board.changeCount == ours else { return }
            board.clearContents()
            if !saved.isEmpty { board.writeObjects(saved) }
        }
    }
}

// MARK: - Window

/// A panel that takes keys without making this app the active one, so the
/// app you're sending to keeps its focus.
final class Panel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }
}

/// Takes every key press in the window and hands it on.
final class KeyView: NSView {
    var onKey: ((NSEvent) -> Void)?
    override var acceptsFirstResponder: Bool { true }
    override func keyDown(with event: NSEvent) { onKey?(event) }
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        // cmd keys come here first; take the ones the window uses
        let cmd = event.modifierFlags.intersection(.deviceIndependentFlagsMask).contains(.command)
        if cmd, ["q", "w"].contains(event.charactersIgnoringModifiers ?? "") || event.keyCode == 36 {
            onKey?(event)
            return true
        }
        return super.performKeyEquivalent(with: event)
    }
    override func mouseDown(with event: NSEvent) { window?.performDrag(with: event) }
}

final class Compose: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private var panel: Panel!
    private let text = NSTextView()
    private let status = NSTextField(labelWithString: "")
    private var engine: Engine?
    private var state: State?
    private var target: NSRunningApplication?
    private var problem: String?
    private let font = NSFont.monospacedSystemFont(ofSize: 15, weight: .regular)
    private let small = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)

    func applicationDidFinishLaunching(_ note: Notification) {
        let me = NSRunningApplication.current
        if let front = NSWorkspace.shared.frontmostApplication, front != me { target = front }
        NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main
        ) { [weak self] note in
            guard let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
                  app != me else { return }
            self?.target = app
            self?.render()
        }
        build()
        start()
        if !Deliver.allowed(prompt: true) {
            problem = "allow Very Normal Compose in System Settings › Privacy & Security › Accessibility, then reopen it"
        }
        render()
    }

    private func start() {
        guard let binary = Engine.locate() else {
            problem = "can't find very-normal: install it, or set VERY_NORMAL_BIN"
            return
        }
        // anything after the app's own arguments (open --args --profile x) goes to very-normal
        let passed = Array(CommandLine.arguments.dropFirst()).filter { !$0.hasPrefix("-psn") }
        do {
            engine = try Engine(binary: binary, arguments: passed) { [weak self] state in
                self?.received(state)
            }
        } catch {
            problem = "couldn't start very-normal: \(error.localizedDescription)"
        }
    }

    private func build() {
        panel = Panel(
            contentRect: NSRect(x: 0, y: 0, width: 560, height: 180),
            styleMask: [.titled, .closable, .resizable, .utilityWindow, .hudWindow, .nonactivatingPanel],
            backing: .buffered, defer: false)
        panel.title = "very-normal"
        panel.level = .floating
        panel.hidesOnDeactivate = false
        panel.isFloatingPanel = true
        panel.becomesKeyOnlyIfNeeded = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.delegate = self
        panel.center()
        panel.setFrameAutosaveName("very-normal compose")

        let root = KeyView()
        root.onKey = { [weak self] in self?.key($0) }

        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.translatesAutoresizingMaskIntoConstraints = false
        text.isEditable = false
        text.isSelectable = false
        text.drawsBackground = false
        text.textContainerInset = NSSize(width: 4, height: 6)
        text.isVerticallyResizable = true
        text.autoresizingMask = [.width]
        text.textContainer?.widthTracksTextView = true
        scroll.documentView = text

        status.font = small
        status.lineBreakMode = .byTruncatingTail
        status.translatesAutoresizingMaskIntoConstraints = false

        root.addSubview(scroll)
        root.addSubview(status)
        NSLayoutConstraint.activate([
            scroll.topAnchor.constraint(equalTo: root.topAnchor, constant: 4),
            scroll.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 8),
            scroll.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -8),
            scroll.bottomAnchor.constraint(equalTo: status.topAnchor, constant: -6),
            status.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 12),
            status.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -12),
            status.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -8),
        ])
        panel.contentView = root
        panel.makeKeyAndOrderFront(nil)
        panel.makeFirstResponder(root)
    }

    // MARK: keys

    private func key(_ event: NSEvent) {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let ctrl = flags.contains(.control), alt = flags.contains(.option)
        let cmd = flags.contains(.command), shift = flags.contains(.shift)
        let plain = event.charactersIgnoringModifiers ?? ""
        problem = nil
        switch event.keyCode {
        case 53: return quit() // esc
        case 36, 76: // return, enter
            if ctrl || alt || cmd {
                engine?.send(["k": "send"])
                pendingReturn = !shift
            } else {
                engine?.send(["k": "char", "c": "\n"])
            }
            return
        case 51: // delete
            if alt || ctrl { engine?.send(["k": "word"]) }
            else if cmd { engine?.send(["k": "clear"]) }
            else { engine?.send(["k": "delete"]) }
            return
        case 48: // tab
            engine?.send(["k": "char", "c": "\t"])
            return
        default: break
        }
        if ctrl {
            switch plain {
            case "c": quit()
            case "w": engine?.send(["k": "word"])
            case "u": engine?.send(["k": "clear"])
            case "j": engine?.send(["k": "char", "c": "\n"])
            default: break
            }
            return
        }
        if cmd {
            if plain == "q" || plain == "w" { quit() }
            return
        }
        for scalar in (event.characters ?? "").unicodeScalars
        where !CharacterSet.controlCharacters.contains(scalar) && scalar.value < 0xF700 {
            engine?.send(["k": "char", "c": String(scalar)])
        }
    }

    /// Whether the send in flight should press Return after the paste.
    private var pendingReturn = true

    private func received(_ state: State) {
        self.state = state
        if let message = state.send {
            if let target, !target.isTerminated, Deliver.allowed(prompt: false) {
                Deliver.paste(message, into: target, pressReturn: pendingReturn)
            } else {
                problem = target == nil
                    ? "click into the app to send to, then come back"
                    : "not allowed to type into other apps (Accessibility)"
            }
        }
        render()
    }

    // MARK: drawing

    private func render() {
        let shown = NSMutableAttributedString()
        let normal: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: NSColor.white]
        let fixed: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: NSColor.systemYellow]
        let dim: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: NSColor.tertiaryLabelColor]
        let segments = state?.segments ?? []
        if segments.isEmpty {
            shown.append(NSAttributedString(string: "▏", attributes: [.font: font, .foregroundColor: NSColor.systemPink]))
            shown.append(NSAttributedString(string: "type a message · ctrl-enter sends it to \(targetName)", attributes: dim))
        } else {
            for s in segments {
                shown.append(NSAttributedString(string: s.t, attributes: s.fixed ? fixed : normal))
            }
            shown.append(NSAttributedString(string: "▏", attributes: [.font: font, .foregroundColor: NSColor.systemPink]))
        }
        text.textStorage?.setAttributedString(shown)
        text.scrollToEndOfDocument(nil)

        let line = NSMutableAttributedString(
            string: "→ \(targetName)",
            attributes: [.font: NSFont.monospacedSystemFont(ofSize: 11, weight: .bold), .foregroundColor: NSColor.systemPink])
        let gray: [NSAttributedString.Key: Any] = [.font: small, .foregroundColor: NSColor.secondaryLabelColor]
        if let problem {
            line.append(NSAttributedString(string: "  \(problem)", attributes: [.font: small, .foregroundColor: NSColor.systemRed]))
        } else if let nudge = state?.nudge {
            line.append(NSAttributedString(string: "  \(nudge)", attributes: [.font: small, .foregroundColor: NSColor.systemYellow]))
        } else {
            let wpm = state?.wpm.map { String(format: "%.0f", $0) } ?? "–"
            let pct = state?.corrected.map { String(format: "%.0f%%", $0) } ?? "–"
            line.append(NSAttributedString(string: "  \(wpm) wpm · \(pct) corrected", attributes: [.font: small, .foregroundColor: NSColor.labelColor]))
        }
        line.append(NSAttributedString(string: "  ·  ctrl-enter send · ctrl-shift-enter paste only · esc close", attributes: gray))
        status.attributedStringValue = line
    }

    private var targetName: String { target?.localizedName ?? "the app you were in" }

    // MARK: closing

    private func quit() {
        engine?.finish()
        NSApp.terminate(nil)
    }

    func windowWillClose(_ notification: Notification) { quit() }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows: Bool) -> Bool {
        panel.makeKeyAndOrderFront(nil)
        return true
    }

    func applicationWillTerminate(_ notification: Notification) { engine?.finish() }
}

let app = NSApplication.shared
let compose = Compose()
app.delegate = compose
app.setActivationPolicy(.accessory)
app.run()
