import SwiftUI

/// The dedicated meeting flow: ready → recording ⇄ paused → preparing → notes.
/// It is full screen because a meeting is a continuing session with an explicit start and stop.
struct MeetingSessionView: View {
    @Environment(AppModel.self) private var model
    @State private var confirmLeave = false

    private var phase: MeetingPhase { model.session?.phase ?? .ready }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                switch phase {
                case .ready: ready
                case .recording, .paused, .interrupted: recording
                case .preparing: preparing
                case .notesFailed: notesFailed
                case .done: done
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .screenBackground()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                if phase != .preparing && phase != .done {
                    ToolbarItem(placement: .cancellationAction) {
                        Button { leave() } label: { Label("Close", systemImage: "xmark") }
                    }
                }
            }
            .confirmationDialog("Leave this meeting?", isPresented: $confirmLeave, titleVisibility: .visible) {
                Button("Keep recording", role: .cancel) {}
                Button("Save unfinished session") { model.saveUnfinishedSession() }
                Button("Discard", role: .destructive) { model.discardSession() }
            } message: {
                Text("Saving keeps what was captured so far. You can finish the notes later.")
            }
        }
    }

    private func leave() {
        switch phase {
        case .ready: model.closeReadySession()
        case .notesFailed: model.saveUnfinishedSession()
        default: confirmLeave = true
        }
    }

    // MARK: Ready

    private var ready: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("talk. get notes.").font(.brandTitle).foregroundStyle(Theme.ink)
                TextField("Meeting title", text: Binding(get: { model.session?.title ?? "" }, set: { model.session?.title = $0 }))
                    .textFieldStyle(.roundedBorder).controlSize(.large)
                    .accessibilityLabel("Meeting title")
                VStack(alignment: .leading, spacing: 8) {
                    Label("Uses your microphone", systemImage: "mic").font(.uiHeadline).foregroundStyle(Theme.ink)
                    Text("Recording starts only when you tap start. It captures people nearby, in person. It can't record phone calls or another app's audio.")
                        .foregroundStyle(Theme.muted)
                    Text("Let everyone in the room know you're recording.").font(.uiSubheadline.weight(.semibold)).foregroundStyle(Theme.ink)
                }
                .card()
                Button {
                    switch model.micAccess {
                    case .granted: model.beginRecording()
                    case .notDetermined: model.micAccess = .granted; model.beginRecording() // prototype stand-in for the system prompt
                    case .denied: model.showToast("microphone access is off. enable it in Settings.")
                    }
                } label: { Label("start recording", systemImage: "record.circle").frame(maxWidth: .infinity) }
                    .buttonStyle(.brandHot)
            }
            .padding(20)
        }
    }

    // MARK: Recording, paused, interrupted

    private var recording: some View {
        VStack(spacing: 16) {
            VStack(spacing: 6) {
                Text(model.session?.title.isEmpty == false ? model.session!.title : "Untitled meeting")
                    .font(.uiHeadline).foregroundStyle(Theme.ink)
                Text(clock(model.session?.elapsed ?? 0))
                    .font(.system(size: 56, weight: .bold, design: .monospaced)).foregroundStyle(Theme.ink)
                    .contentTransition(.numericText())
                statusLabel
            }
            .accessibilityElement(children: .combine)
            Waveform(active: phase == .recording)
            transcript
            controls
        }
        .padding(20)
    }

    @ViewBuilder private var statusLabel: some View {
        switch phase {
        case .recording: Label("recording", systemImage: "record.circle.fill").font(.brandMono).foregroundStyle(Theme.error)
        case .paused: Label("paused. take your time.", systemImage: "pause.circle").font(.brandMono).foregroundStyle(Theme.ink)
        default: Label("recording paused", systemImage: "exclamationmark.circle").font(.brandMono).foregroundStyle(Theme.ink)
        }
    }

    private var transcript: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    if phase == .interrupted {
                        Text("Something interrupted the microphone at \(clock(model.session?.elapsed ?? 0)). We kept \(model.session?.lines.count ?? 0) lines. Nothing was recorded since.")
                            .font(.uiSubheadline).foregroundStyle(Theme.ink).card()
                    }
                    ForEach(model.session?.lines ?? []) { line in
                        HStack(alignment: .top, spacing: 10) {
                            Text(clock(line.time)).font(.brandMono).foregroundStyle(Theme.muted)
                            Text(line.text).foregroundStyle(Theme.ink)
                        }
                        .id(line.id)
                    }
                    if (model.session?.lines.isEmpty ?? true) && phase == .recording {
                        Text("listening for the first words…").foregroundStyle(Theme.muted)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .onChange(of: model.session?.lines.count) {
                if let last = model.session?.lines.last { proxy.scrollTo(last.id, anchor: .bottom) }
            }
        }
        .frame(maxHeight: .infinity)
    }

    /// Fixed at the bottom so a growing transcript never moves the controls.
    @ViewBuilder private var controls: some View {
        switch phase {
        case .recording:
            HStack(spacing: 12) {
                Button { model.pauseMeeting() } label: { Label("pause", systemImage: "pause.fill").frame(maxWidth: .infinity) }
                    .buttonStyle(.brandQuiet)
                Button { model.finishMeeting() } label: { Label("finish", systemImage: "checkmark").frame(maxWidth: .infinity) }
                    .buttonStyle(.brandPrimary)
            }.controlSize(.large)
        case .paused:
            HStack(spacing: 12) {
                Button { model.resumeMeeting() } label: { Label("resume", systemImage: "play.fill").frame(maxWidth: .infinity) }
                    .buttonStyle(.brandPrimary)
                Button { model.finishMeeting() } label: { Label("finish", systemImage: "checkmark").frame(maxWidth: .infinity) }
                    .buttonStyle(.brandQuiet)
            }.controlSize(.large)
        default:
            VStack(spacing: 12) {
                Button { model.resumeMeeting() } label: { Text("resume").frame(maxWidth: .infinity) }
                    .buttonStyle(.brandPrimary)
                Button { model.finishMeeting() } label: { Text("finish with what we have").frame(maxWidth: .infinity) }
                    .buttonStyle(.brandQuiet)
            }.controlSize(.large)
        }
    }

    // MARK: Notes

    private var preparing: some View {
        VStack(spacing: 16) {
            ProgressView().controlSize(.large)
            Text("writing up your notes…").font(.brandHeading).foregroundStyle(Theme.ink)
            Text("\(model.session?.title.isEmpty == false ? model.session!.title : "Untitled meeting") · \(clock(model.session?.elapsed ?? 0))")
                .font(.brandMono).foregroundStyle(Theme.muted)
        }
        .padding(20)
        .accessibilityElement(children: .combine)
    }

    // The transcript is intact; only the write-up failed, and the screen says so.
    private var notesFailed: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("couldn't write the notes.").font(.brandHeading).foregroundStyle(Theme.ink)
            Text("Your \(clock(model.session?.elapsed ?? 0)) and \(model.session?.lines.count ?? 0) lines are safe.").foregroundStyle(Theme.muted)
            ScrollView {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(model.session?.lines ?? []) { line in
                        HStack(alignment: .top, spacing: 10) {
                            Text(clock(line.time)).font(.brandMono).foregroundStyle(Theme.muted)
                            Text(line.text).foregroundStyle(Theme.ink)
                        }
                    }
                }.frame(maxWidth: .infinity, alignment: .leading)
            }
            Button { model.retryNotes() } label: { Text("try again").frame(maxWidth: .infinity) }
                .buttonStyle(.brandPrimary).controlSize(.large)
            Button { model.saveUnfinishedSession() } label: { Text("save and finish later").frame(maxWidth: .infinity) }
                .buttonStyle(.brandQuiet).controlSize(.large)
        }
        .padding(20)
    }

    private var done: some View {
        VStack(spacing: 16) {
            PixelSpark(color: Theme.orange, cell: 8)
            Text("notes are ready.").font(.brandTitle).foregroundStyle(Theme.ink)
            Button { model.openFinishedMeeting() } label: { Text("open notes").frame(maxWidth: .infinity) }
                .buttonStyle(.brandPrimary).controlSize(.large)
        }
        .padding(20)
    }
}
