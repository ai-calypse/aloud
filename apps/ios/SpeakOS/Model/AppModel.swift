import SwiftUI

@MainActor @Observable
final class AppModel {
    // MARK: Content
    var items: [MemoryItem] = []
    var meetings: [Meeting] = []
    var connections: [Connection] = Fixtures.connections()

    // MARK: Preferences
    var broMode = false
    var haptics = true
    var tapToRecord = false
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

    // MARK: Capture
    var phase: CapturePhase = .idle
    var liveTranscript = ""
    var processingSlow = false
    var lastSaved: MemoryItem?
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
        lastSaved = nil
    }

    func warm(_ plain: String, bro: String) -> String { broMode ? bro : plain }

    // MARK: Derived

    var recent: [MemoryItem] { Array(items.sorted { $0.date > $1.date }.prefix(3)) }

    var results: [MemoryItem] {
        let q = query.trimmingCharacters(in: .whitespaces)
        return items
            .filter { filter.kind == nil || $0.kind == filter.kind }
            .filter { q.isEmpty || ($0.title + " " + $0.body + " " + $0.tags.joined(separator: " ")).localizedCaseInsensitiveContains(q) }
            .sorted { ($0.pinned ? 1 : 0, $0.date) > ($1.pinned ? 1 : 0, $1.date) }
    }

    func item(_ id: UUID) -> MemoryItem? { items.first { $0.id == id } }
    func meeting(_ id: UUID) -> Meeting? { meetings.first { $0.id == id } }
    func connection(for service: ServiceKind) -> Connection { connections.first { $0.service == service }! }

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
        lastSaved = nil
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
        // A brush of the control is a no-op, never an empty saved thought.
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
        case .thought(let t): saveThought(t)
        case .draft(let t, let name): present(.draft(DraftModel(text: t, recipientName: name)))
        case .action(let request): startAction(request)
        case .answer(let answer): present(.answer(answer))
        case .unclear: present(.clarify(Clarification(text: text)))
        }
    }

    // MARK: Thoughts and drafts

    func saveThought(_ text: String) {
        let item = MemoryItem(kind: .thought, title: String(text.prefix(60)), body: text)
        items.insert(item, at: 0)
        lastSaved = item
        if haptics { Haptics.success() }
        AccessibilityNotification.Announcement("remembered. \(text)").post()
    }

    func undoLastSaved() {
        guard let saved = lastSaved else { return }
        items.removeAll { $0.id == saved.id }
        lastSaved = nil
        showToast("undone. it's gone.")
    }

    func updateItem(_ id: UUID, title: String, body: String) {
        guard let i = items.firstIndex(where: { $0.id == id }) else { return }
        items[i].title = title
        items[i].body = body
        if lastSaved?.id == id { lastSaved = items[i] }
    }

    func togglePin(_ id: UUID) {
        guard let i = items.firstIndex(where: { $0.id == id }) else { return }
        items[i].pinned.toggle()
    }

    func deleteItem(_ id: UUID) {
        guard let item = item(id) else { return }
        items.removeAll { $0.id == id }
        if let m = item.meetingID { meetings.removeAll { $0.id == m } }
        if lastSaved?.id == id { lastSaved = nil }
        showToast("forgotten.")
    }

    @discardableResult
    func saveDraft(_ text: String) -> UUID {
        let item = MemoryItem(kind: .draft, title: String(text.prefix(60)), body: text)
        items.insert(item, at: 0)
        showToast("draft saved.")
        return item.id
    }

    func deleteAll() {
        items = []
        meetings = []
        lastSaved = nil
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

    func setStatus(_ status: ConnectionStatus, for service: ServiceKind) {
        guard let i = connections.firstIndex(where: { $0.service == service }) else { return }
        connections[i].status = status
        if status == .connected, connections[i].account.isEmpty {
            connections[i].account = service == .notion ? "Personal workspace" : "you@example.com"
        }
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
    func connect(_ service: ServiceKind) async -> ConnectionResult {
        try? await Task.sleep(for: .milliseconds(1300))
        let result = demo.connectionResult
        if result == .connected { setStatus(.connected, for: service) }
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
