import Foundation

// Prototype value types. Nothing here talks to a network, microphone or account:
// every outcome is a simulated fixture (see AppModel and DemoControls).

enum MemoryKind: String, CaseIterable, Identifiable, Hashable {
    case note, meeting, action

    var id: String { rawValue }
    var label: String { rawValue }
    var symbol: String {
        switch self {
        case .note: "note.text"
        case .meeting: "person.2"
        case .action: "paperplane"
        }
    }
}

enum MemoryFilter: String, CaseIterable, Identifiable {
    case everything, notes, meetings, actions

    var id: String { rawValue }
    var kind: MemoryKind? {
        switch self {
        case .everything: nil
        case .notes: .note
        case .meetings: .meeting
        case .actions: .action
        }
    }
}

enum Route: Hashable {
    case memory(UUID)
    case meeting(UUID)
}

struct MemoryItem: Identifiable, Hashable {
    var id = UUID()
    var kind: MemoryKind
    var title: String
    var body: String
    var date: Date = .now
    var pinned = false
    var tags: [String] = []
    var meetingID: UUID?

    var route: Route {
        if let meetingID { return .meeting(meetingID) }
        return .memory(id)
    }
}

// MARK: Meetings

struct TranscriptLine: Identifiable, Hashable {
    var id = UUID()
    var time: Int
    var text: String
}

enum MeetingStatus: Hashable {
    case finished
    /// Capture stopped at `stoppedAt` seconds and notes were never prepared.
    case unfinished(stoppedAt: Int)
}

struct Meeting: Identifiable, Hashable {
    var id = UUID()
    var title: String
    var date: Date
    var duration: Int
    var summary: String
    var decisions: [String]
    var actionItems: [String]
    var transcript: [TranscriptLine]
    var status: MeetingStatus = .finished

    var notesText: String {
        var parts = ["\(title)", summary]
        if !decisions.isEmpty { parts.append("Decisions:\n" + decisions.map { "- \($0)" }.joined(separator: "\n")) }
        if !actionItems.isEmpty { parts.append("Action items:\n" + actionItems.map { "- \($0)" }.joined(separator: "\n")) }
        return parts.joined(separator: "\n\n")
    }

    /// ponytail: keyword rules stand in for the note-writing model. The wording is
    /// what matters to the UI: nothing is invented, and empty sections stay empty.
    static func compose(from lines: [TranscriptLine], duration: Int) -> (summary: String, decisions: [String], actions: [String]) {
        guard !lines.isEmpty else {
            return ("Nothing was captured, so there is nothing to summarise.", [], [])
        }
        let decisions = lines.map(\.text).filter { $0.localizedCaseInsensitiveContains("agreed") || $0.localizedCaseInsensitiveContains("decided") || $0.localizedCaseInsensitiveContains("lock") }
        let actions = lines.map(\.text).filter { $0.localizedCaseInsensitiveContains(" will ") || $0.localizedCaseInsensitiveContains("need to") }
        let minutes = max(1, duration / 60)
        let summary = "\(lines.count) points over about \(minutes) min. \(lines[0].text)"
        return (summary, decisions, actions)
    }
}

enum MeetingPhase: Equatable {
    case ready, recording, paused, interrupted, preparing, notesFailed, done
}

struct MeetingSession {
    var title = ""
    var phase: MeetingPhase = .ready
    var elapsed = 0
    var lines: [TranscriptLine] = []
    var finishedID: UUID?
}

// MARK: Services and actions

enum ServiceKind: String, CaseIterable, Identifiable, Hashable {
    case slack, gmail, notion

    var id: String { rawValue }
    var name: String {
        switch self {
        case .slack: "Slack"
        case .gmail: "Gmail"
        case .notion: "Notion"
        }
    }
    var symbol: String {
        switch self {
        case .slack: "number.square"
        case .gmail: "envelope"
        case .notion: "doc.text"
        }
    }
    /// The button names what actually happens: Notion creates, it never "sends".
    var verb: String { self == .notion ? "create note" : "send" }
    var readyLine: String { self == .notion ? "ready to create" : "ready to send" }
    var doneLine: String { self == .notion ? "note created" : "sent" }
    var destinationLabel: String { self == .notion ? "Where" : "To" }
    var access: [String] {
        switch self {
        case .slack: ["Send messages you have reviewed to people and channels you pick.", "Look up names so we can show who a message goes to."]
        case .gmail: ["Create drafts and send emails you have reviewed.", "Look up contacts so we can show who an email goes to."]
        case .notion: ["Create notes in the page you pick.", "Read page names so we can show where a note will go."]
        }
    }
}

enum ConnectionStatus: String, CaseIterable, Identifiable {
    case connected, expired, notConnected
    var id: String { rawValue }
    var label: String {
        switch self {
        case .connected: "Connected"
        case .expired: "Needs reconnecting"
        case .notConnected: "Not connected"
        }
    }
}

enum IntegrationCategory: String, CaseIterable, Identifiable {
    case messaging, email, docs, calendar, tasks, dev, sales
    var id: String { rawValue }
    var label: String {
        switch self {
        case .messaging: "Messaging"
        case .email: "Email"
        case .docs: "Notes & docs"
        case .calendar: "Calendar"
        case .tasks: "Tasks"
        case .dev: "Developer"
        case .sales: "Sales"
        }
    }
}

/// One thing an app connection can do. Writes always ask first; reads can be allowed.
struct IntegrationTool: Identifiable, Hashable {
    var name: String
    var writes: Bool
    var asksFirst: Bool
    var id: String { name }
}

/// A connectable app. Connections run through Composio, so this list is a catalogue of
/// toolkits and the status of the person's link to each, never a credential.
struct Integration: Identifiable {
    var id: String
    var name: String
    var symbol: String
    var category: IntegrationCategory
    var summary: String
    var tools: [IntegrationTool]
    var status: ConnectionStatus = .notConnected
    var account: String = ""
}

struct Recipient: Identifiable, Hashable {
    var name: String
    var detail: String
    var id: String { name + detail }
}

struct ActionRequest: Identifiable, Hashable {
    var id = UUID()
    var service: ServiceKind
    var content: String
    var recipient: Recipient?
    var candidates: [Recipient]
}

// MARK: Results

/// What the note editor opens with: fresh dictation, or a saved note being edited.
struct NoteDraft: Identifiable {
    var id = UUID()
    var text: String
    var recipientName: String?
    var existingID: UUID?
    var tags: [String] = []
    /// A default tone the person set in Settings; the editor opens already rewritten.
    var style: RewriteStyle?
}

struct TextReplacement: Identifiable, Hashable {
    var id = UUID()
    var from: String
    var to: String
}

enum SpeechLanguage: String, CaseIterable, Identifiable {
    case enUS = "English (US)", enGB = "English (UK)", es = "Español", fr = "Français", de = "Deutsch", hi = "हिन्दी", pt = "Português"
    var id: String { rawValue }
}

enum AnswerState {
    case answer(String)
    case noMatch
    case unavailable
}

struct AnswerModel: Identifiable {
    var id = UUID()
    var question: String
    var state: AnswerState
    var sources: [UUID] = []
}

enum MicAccess: String, CaseIterable, Identifiable {
    case notDetermined, granted, denied
    var id: String { rawValue }
    var label: String {
        switch self {
        case .notDetermined: "Ask on first use"
        case .granted: "Allowed"
        case .denied: "Denied"
        }
    }
}

enum CapturePhase: Equatable {
    case idle
    case listening(armed: Bool)
    case processing(String)
    case failed(String)
}

enum ActionOutcome: String, CaseIterable, Identifiable {
    case success, failure, unknown
    var id: String { rawValue }
    var label: String {
        switch self {
        case .success: "Succeeds"
        case .failure: "Fails once"
        case .unknown: "Outcome unknown"
        }
    }
}

enum ConnectionResult: String, CaseIterable, Identifiable {
    case connected, canceled, failed
    var id: String { rawValue }
    var label: String {
        switch self {
        case .connected: "Connects"
        case .canceled: "Is canceled"
        case .failed: "Fails"
        }
    }
}

/// Switches that make every state in the guide reachable without real services.
struct DemoControls {
    var nextSample = 0
    var nextCaptureFails = false
    var searchUnavailable = false
    var actionOutcome: ActionOutcome = .success
    var connectionResult: ConnectionResult = .connected
    var interruptMeeting = false
    var notesFail = false
}
