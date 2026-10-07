import SwiftUI

@MainActor @Observable
final class AppModel {
    // MARK: Content
    var items: [MemoryItem] = []
    var meetings: [Meeting] = []
    var integrations: [Integration] = Fixtures.integrations()

    // MARK: Preferences
    var broMode = false
    var haptics = true
    var tapToRecord = false
    var language: SpeechLanguage = .enUS
    var defaultTone: RewriteStyle?
    var keepAudio = false
    var account: String?
    var replacements: [TextReplacement] = [TextReplacement(from: "my email", to: "you@example.com")]
    var micAccess: MicAccess = .notDetermined
    var demo = DemoControls()

    // MARK: Navigation
    var selectedTab: AppTab = .speak
    var speakPath: [Route] = []
    var memoryPath: [Route] = []
    var meetingsPath: [Route] = []
    var sheet: AppSheet?
    var toastText: String?

    // Search state lives here so a detail view and back keeps the query and filter.
    var query = ""
    var filter: MemoryFilter = .everything
    var tagFilter: String?

    // MARK: Capture
    var phase: CapturePhase = .idle
    var liveTranscript = ""
    var processingSlow = false
    var session: MeetingSession?

    @ObservationIgnored private var listenTask: Task<Void, Never>?
    @ObservationIgnored private var processTask: Task<Void, Never>?
    @ObservationIgnored private var toastTask: Task<Void, Never>?
    @ObservationIgnored private var meetingTask: Task<Void, Never>?
    @ObservationIgnored private var captureStart = Date.now

    init() { applyFixtures(firstUse: false) }

    func applyFixtures(firstUse: Bool) {
        if firstUse {
            items = []
            meetings = []
        } else {
            meetings = Fixtures.meetings()
            items = Fixtures.items(meetings: meetings)
        }
    }

    func warm(_ plain: String, bro: String) -> String { broMode ? bro : plain }

    // MARK: Derived

    var recent: [MemoryItem] { Array(items.sorted { $0.date > $1.date }.prefix(3)) }

    /// Memory applies its kind filter; pages without filter chips use `searchResults`.
    var results: [MemoryItem] {
        searchResults
            .filter { filter.kind == nil || $0.kind == filter.kind }
            .filter { tagFilter == nil || $0.tags.contains(tagFilter!) }
    }

    var searchResults: [MemoryItem] {
        let q = query.trimmingCharacters(in: .whitespaces)
        return items
            .filter { q.isEmpty || ($0.title + " " + $0.body + " " + $0.tags.joined(separator: " ")).localizedCaseInsensitiveContains(q) }
            .sorted { ($0.pinned ? 1 : 0, $0.date) > ($1.pinned ? 1 : 0, $1.date) }
    }

    /// Meetings stay their own list; the shared query narrows it by title, notes and transcript.
    var visibleMeetings: [Meeting] {
        let q = query.trimmingCharacters(in: .whitespaces)
        return meetings
            .filter { q.isEmpty || ($0.title + " " + $0.summary + " " + $0.transcript.map(\.text).joined(separator: " ")).localizedCaseInsensitiveContains(q) }
            .sorted { $0.date > $1.date }
    }

    /// Notes that share a meaningful word with this one, for the "from your memory" strip.
    func related(to item: MemoryItem, limit: Int = 2) -> [MemoryItem] {
        let words = Set(Interpreter.words((item.title + " " + item.body).lowercased()).filter { $0.count >= 5 })
        guard !words.isEmpty else { return [] }
        return items
            .filter { $0.id != item.id && $0.kind != .action }
            .map { other in (other, Interpreter.words((other.title + " " + other.body).lowercased()).filter { words.contains($0) }.count) }
            .filter { $0.1 > 0 }
            .sorted { ($0.1, $0.0.date) > ($1.1, $1.0.date) }
            .prefix(limit).map(\.0)
    }

    func item(_ id: UUID) -> MemoryItem? { items.first { $0.id == id } }
    func meeting(_ id: UUID) -> Meeting? { meetings.first { $0.id == id } }
    func integration(_ id: String) -> Integration? { integrations.first { $0.id == id } }
    func connection(for service: ServiceKind) -> Integration { integration(service.id)! }
    var connectedCount: Int { integrations.filter { $0.status == .connected }.count }

    var exportText: String {
        items.sorted { $0.date < $1.date }.map { "[\($0.kind.label)] \($0.title)\n\($0.body)" }.joined(separator: "\n\n")
    }

    // MARK: Feedback

    func showToast(_ text: String) {
        toastText = text
        AccessibilityNotification.Announcement(text).post()
        toastTask?.cancel()
        toastTask = Task { [self] in
            try? await Task.sleep(for: .seconds(2.4))
            if !Task.isCancelled { toastText = nil }
        }
    }

    // MARK: Sheets

    /// One sheet at a time: close the current one, then open the next.
    func present(_ next: AppSheet) {
        guard sheet != nil else { sheet = next; return }
        sheet = nil
        Task { [self] in
            try? await Task.sleep(for: .milliseconds(450))
            sheet = next
        }
    }

    // MARK: Short capture (voice or typed)

    func beginCapture() {
        guard phase == .idle else { return }
        switch micAccess {
        case .notDetermined: present(.permission(denied: false)); return
        case .denied: present(.permission(denied: true)); return
        case .granted: break
        }
        liveTranscript = ""
        captureStart = .now
        phase = .listening(armed: false)
        if haptics { Haptics.start() }
        let sample = DemoScript.samples[demo.nextSample]
        demo.nextSample = (demo.nextSample + 1) % DemoScript.samples.count
        let words = sample.split(separator: " ").map(String.init)
        listenTask = Task { [self] in
            for word in words {
                try? await Task.sleep(for: .milliseconds(240))
                if Task.isCancelled { return }
                liveTranscript += (liveTranscript.isEmpty ? "" : " ") + word
            }
        }
    }

    func setArmed(_ armed: Bool) {
        guard case .listening(let current) = phase, current != armed else { return }
        phase = .listening(armed: armed)
        if haptics { Haptics.tick() }
    }

    func toggleTapRecording() {
        if case .listening = phase { releaseCapture() } else { beginCapture() }
    }

    func releaseCapture() {
        guard case .listening(let armed) = phase else { return }
        listenTask?.cancel()
        if armed { cancelCapture(); return }
        let text = liveTranscript.trimmingCharacters(in: .whitespaces)
        // A brush of the control is a no-op, never an empty note.
        if text.isEmpty || (!tapToRecord && Date.now.timeIntervalSince(captureStart) < 0.4) {
            phase = .idle
            liveTranscript = ""
            showToast(warm("hold a little longer, then let go.", bro: "hold it, bro. then let go."))
            return
        }
        if haptics { Haptics.tick() }
        submit(text)
    }

    func cancelCapture() {
        listenTask?.cancel()
        phase = .idle
        liveTranscript = ""
        showToast("scrapped. all good.")
    }

    func submit(_ text: String) {
        selectedTab = .speak
        processTask?.cancel()
        processingSlow = false
        phase = .processing(text)
        let fails = demo.nextCaptureFails
        processTask = Task { [self] in
            do {
                if fails {
                    try await Task.sleep(for: .milliseconds(2600))
                    processingSlow = true
                    try await Task.sleep(for: .milliseconds(900))
                    demo.nextCaptureFails = false
                    processingSlow = false
                    phase = .failed(text)
                } else {
                    try await Task.sleep(for: .milliseconds(1800))
                    resolve(text)
                }
            } catch {}
        }
    }

    func cancelProcessing() {
        processTask?.cancel()
        processingSlow = false
        phase = .idle
        showToast("scrapped. all good.")
    }

    func discardFailed() {
        phase = .idle
        liveTranscript = ""
    }

    private func resolve(_ text: String) {
        phase = .idle
        liveTranscript = ""
        switch Interpreter.interpret(text, items: items, searchUnavailable: demo.searchUnavailable) {
        case .note(let t, let name): present(.note(NoteDraft(text: applyReplacements(t), recipientName: name, existingID: nil, tags: [], style: defaultTone)))
        case .answer(let answer): present(.answer(answer))
        }
    }

    // MARK: Tags

    nonisolated static func cleanTag(_ raw: String) -> String {
        raw.trimmingCharacters(in: .whitespacesAndNewlines)
            .trimmingCharacters(in: CharacterSet(charactersIn: "#"))
            .lowercased()
            .split(whereSeparator: { $0.isWhitespace }).joined(separator: "-")
    }

    var allTags: [(name: String, count: Int)] {
        var counts: [String: Int] = [:]
        for t in items.flatMap(\.tags) { counts[t, default: 0] += 1 }
        return counts.map { (name: $0.key, count: $0.value) }.sorted { ($0.count, $1.name) > ($1.count, $0.name) }
    }

    func setTags(_ id: UUID, _ tags: [String]) {
        guard let i = items.firstIndex(where: { $0.id == id }) else { return }
        var seen = Set<String>()
        items[i].tags = tags.map(Self.cleanTag).filter { !$0.isEmpty && seen.insert($0).inserted }
    }

    /// Renaming onto an existing tag merges the two.
    func renameTag(_ old: String, to new: String) {
        let target = Self.cleanTag(new)
        guard !target.isEmpty, target != old else { return }
        for i in items.indices where items[i].tags.contains(old) {
            var tags = items[i].tags.map { $0 == old ? target : $0 }
            var seen = Set<String>()
            tags = tags.filter { seen.insert($0).inserted }
            items[i].tags = tags
        }
        if tagFilter == old { tagFilter = target }
    }

    func deleteTag(_ name: String) {
        for i in items.indices { items[i].tags.removeAll { $0 == name } }
        if tagFilter == name { tagFilter = nil }
    }

    /// Existing tags whose word appears in the text, then the most used, minus ones already applied.
    func suggestTags(for text: String, excluding applied: [String], limit: Int = 6) -> [String] {
        let words = Set(Interpreter.words(text.lowercased()))
        let mentioned = allTags.map(\.name).filter { words.contains($0) }
        let popular = allTags.map(\.name)
        var out: [String] = []
        for t in mentioned + popular where !applied.contains(t) && !out.contains(t) { out.append(t) }
        return Array(out.prefix(limit))
    }

    // MARK: Notes

    /// Saves a new note, or updates the one being edited. Returns its id.
    @discardableResult
    func saveNote(id: UUID?, text: String, tags: [String] = []) -> UUID {
        let title = String(text.trimmingCharacters(in: .whitespacesAndNewlines).prefix(60))
        if let id, let i = items.firstIndex(where: { $0.id == id }) {
            items[i].title = title
            items[i].body = text
            items[i].tags = tags
            showToast("note updated.")
            return id
        }
        let item = MemoryItem(kind: .note, title: title, body: text, tags: tags)
        items.insert(item, at: 0)
        if haptics { Haptics.success() }
        showToast("saved to Memory.")
        return item.id
    }

    /// Your shortcuts ("my email" → an address) are applied to what you dictate, never to saved notes.
    func applyReplacements(_ text: String) -> String {
        replacements.reduce(text) { out, r in
            r.from.isEmpty ? out : out.replacingOccurrences(of: r.from, with: r.to, options: .caseInsensitive)
        }
    }

    func rewrite(_ text: String, style: RewriteStyle) async -> String {
        try? await Task.sleep(for: .milliseconds(800))
        return Rewriter.apply(text, style: style)
    }

    func togglePin(_ id: UUID) {
        guard let i = items.firstIndex(where: { $0.id == id }) else { return }
        items[i].pinned.toggle()
    }

    func deleteItem(_ id: UUID) {
        guard let item = self.item(id) else { return }
        items.removeAll { $0.id == id }
        if let m = item.meetingID { meetings.removeAll { $0.id == m } }
        showToast("forgotten.")
    }

    func deleteAll() {
        items = []
        meetings = []
        showToast("everything's forgotten.")
    }

    // MARK: Connected actions

    func startAction(_ request: ActionRequest) {
        if request.recipient == nil { present(.recipient(request)) } else { proceed(request) }
    }

    func choose(_ recipient: Recipient, for request: ActionRequest) {
        var r = request
        r.recipient = recipient
        proceed(r)
    }

    func proceed(_ request: ActionRequest) {
        if connection(for: request.service).status == .connected {
            present(.action(request))
        } else {
            present(.connect(request.service, resume: request))
        }
    }

    func setStatus(_ status: ConnectionStatus, for service: ServiceKind) { setStatus(status, forID: service.id) }

    func setStatus(_ status: ConnectionStatus, forID id: String) {
        guard let i = integrations.firstIndex(where: { $0.id == id }) else { return }
        integrations[i].status = status
        if status == .connected, integrations[i].account.isEmpty {
            integrations[i].account = id == "notion" ? "Personal workspace" : "you@example.com"
        }
    }

    func setAsksFirst(_ asks: Bool, tool: String, in id: String) {
        guard let i = integrations.firstIndex(where: { $0.id == id }), let t = integrations[i].tools.firstIndex(where: { $0.name == tool }) else { return }
        // Writes always ask: the switch only exists for reads.
        integrations[i].tools[t].asksFirst = asks || integrations[i].tools[t].writes
    }

    enum SendResult { case sent, failed, unknown }

    func performSend(_ request: ActionRequest) async -> SendResult {
        try? await Task.sleep(for: .milliseconds(1400))
        switch demo.actionOutcome {
        case .success:
            recordAction(request)
            return .sent
        case .failure:
            demo.actionOutcome = .success
            return .failed
        case .unknown:
            demo.actionOutcome = .success
            return .unknown
        }
    }

    /// Asking again is a status check, never a second send.
    func checkStatus(_ request: ActionRequest) async -> SendResult {
        try? await Task.sleep(for: .milliseconds(1500))
        recordAction(request)
        return .sent
    }

    private func recordAction(_ request: ActionRequest) {
        let title = request.service == .notion
            ? "Note created in Notion"
            : "Sent to \(request.recipient?.name ?? "someone") on \(request.service.name)"
        items.insert(MemoryItem(kind: .action, title: title, body: request.content), at: 0)
        if haptics { Haptics.success() }
    }

    /// Attempt to link a service. Returns the simulated result.
    func connect(_ id: String) async -> ConnectionResult {
        try? await Task.sleep(for: .milliseconds(1300))
        let result = demo.connectionResult
        if result == .connected { setStatus(.connected, forID: id) }
        return result
    }

    // MARK: Meetings

    func startMeeting() {
        guard session == nil else { return }
        session = MeetingSession()
    }

    func beginRecording() {
        guard session?.phase == .ready else { return }
        session?.phase = .recording
        if haptics { Haptics.start() }
        runClock()
    }

    func pauseMeeting() { session?.phase = .paused }
    func resumeMeeting() { session?.phase = .recording }

    private func runClock() {
        meetingTask?.cancel()
        meetingTask = Task { [self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard !Task.isCancelled, session?.phase == .recording else { continue }
                session?.elapsed += 1
                let t = session?.elapsed ?? 0
                if t % 4 == 0 {
                    let lines = DemoScript.meetingLines
                    session?.lines.append(TranscriptLine(time: t, text: lines[(session?.lines.count ?? 0) % lines.count]))
                }
                if demo.interruptMeeting && t >= 8 {
                    demo.interruptMeeting = false
                    session?.phase = .interrupted
                }
            }
        }
    }

    func finishMeeting() {
        guard var s = session else { return }
        meetingTask?.cancel()
        s.phase = .preparing
        session = s
        Task { [self] in
            try? await Task.sleep(for: .milliseconds(2000))
            guard session?.phase == .preparing else { return }
            if demo.notesFail {
                demo.notesFail = false
                session?.phase = .notesFailed
                return
            }
            completeSession()
        }
    }

    func retryNotes() { finishMeeting() }

    private func completeSession() {
        guard let s = session else { return }
        let notes = Meeting.compose(from: s.lines, duration: s.elapsed)
        let meeting = Meeting(
            title: s.title.isEmpty ? "Untitled meeting" : s.title,
            date: .now, duration: s.elapsed, summary: notes.summary,
            decisions: notes.decisions, actionItems: notes.actions, transcript: s.lines
        )
        meetings.insert(meeting, at: 0)
        items.insert(MemoryItem(kind: .meeting, title: meeting.title, body: meeting.summary, meetingID: meeting.id), at: 0)
        session?.finishedID = meeting.id
        session?.phase = .done
        if haptics { Haptics.success() }
    }

    func openFinishedMeeting() {
        guard let id = session?.finishedID else { return }
        session = nil
        selectedTab = .meetings
        meetingsPath = [.meeting(id)]
    }

    func saveUnfinishedSession() {
        guard let s = session else { return }
        meetingTask?.cancel()
        let meeting = Meeting(
            title: s.title.isEmpty ? "Untitled meeting" : s.title,
            date: .now, duration: s.elapsed, summary: "", decisions: [], actionItems: [],
            transcript: s.lines, status: .unfinished(stoppedAt: s.elapsed)
        )
        meetings.insert(meeting, at: 0)
        session = nil
        showToast("saved. you can finish it later.")
    }

    func discardSession() {
        meetingTask?.cancel()
        session = nil
    }

    func closeReadySession() { session = nil }

    func finishRecovered(_ id: UUID) async {
        try? await Task.sleep(for: .milliseconds(1500))
        guard let i = meetings.firstIndex(where: { $0.id == id }) else { return }
        let notes = Meeting.compose(from: meetings[i].transcript, duration: meetings[i].duration)
        meetings[i].summary = notes.summary
        meetings[i].decisions = notes.decisions
        meetings[i].actionItems = notes.actions
        meetings[i].status = .finished
        items.insert(MemoryItem(kind: .meeting, title: meetings[i].title, body: notes.summary, date: meetings[i].date, meetingID: id), at: 0)
        if haptics { Haptics.success() }
    }

    func deleteMeeting(_ id: UUID) {
        meetings.removeAll { $0.id == id }
        items.removeAll { $0.meetingID == id }
        showToast("forgotten.")
    }
}
