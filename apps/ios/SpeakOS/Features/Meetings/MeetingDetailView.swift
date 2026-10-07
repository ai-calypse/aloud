import SwiftUI

struct MeetingDetailView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let meetingID: UUID

    @State private var preparing = false
    @State private var confirmDiscard = false

    var body: some View {
        Group {
            if let meeting = model.meeting(meetingID) {
                content(meeting)
            } else {
                ContentUnavailableView("forgotten.", systemImage: "trash", description: Text("This meeting is gone."))
            }
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
    }

    private func content(_ meeting: Meeting) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(meeting.title).font(.brandTitle).foregroundStyle(Theme.ink)
                    Text("\(meeting.date.formatted(date: .abbreviated, time: .shortened)) · \(clock(meeting.duration))")
                        .font(.brandMono).foregroundStyle(Theme.muted)
                }
                switch meeting.status {
                case .unfinished(let at): recovery(meeting, stoppedAt: at)
                case .finished: notes(meeting)
                }
                transcript(meeting)
            }
            .padding(20)
        }
        .toolbar {
            if meeting.status == .finished {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    ShareLink(item: meeting.notesText)
                    Menu {
                        ForEach(ServiceKind.allCases) { service in
                            Button { sendNotes(meeting, via: service) } label: { Label(service.name, systemImage: service.symbol) }
                        }
                    } label: { Label("Send notes with…", systemImage: "paperplane") }
                }
            }
        }
        .confirmationDialog("Discard this session?", isPresented: $confirmDiscard, titleVisibility: .visible) {
            Button("Discard session", role: .destructive) { model.deleteMeeting(meeting.id); dismiss() }
            Button("Keep it", role: .cancel) {}
        } message: {
            Text("The captured transcript will be gone for good.")
        }
    }

    // MARK: Notes

    @ViewBuilder private func notes(_ meeting: Meeting) -> some View {
        section("summary", empty: "No summary was written.", lines: meeting.summary.isEmpty ? [] : [meeting.summary])
        section("decisions", empty: "No decisions came up.", lines: meeting.decisions, bullets: true)
        section("action items", empty: "No action items came up.", lines: meeting.actionItems, bullets: true)
        // The wording is the model's reading of the transcript, so say so.
        Text("Notes are written from the transcript below. Check them against it.").font(.uiFootnote).foregroundStyle(Theme.muted)
    }

    private func section(_ title: String, empty: String, lines: [String], bullets: Bool = false) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).font(.brandHeading).foregroundStyle(Theme.ink).accessibilityAddTraits(.isHeader)
            if lines.isEmpty {
                Text(empty).foregroundStyle(Theme.muted)
            } else {
                ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                    HStack(alignment: .top, spacing: 8) {
                        if bullets { Text("•").foregroundStyle(Theme.primary).accessibilityHidden(true) }
                        Text(line).foregroundStyle(Theme.ink)
                    }
                }
            }
        }
        .card()
    }

    // MARK: Recovery (F07)

    private func recovery(_ meeting: Meeting, stoppedAt: Int) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("this session didn't finish.", systemImage: "exclamationmark.circle").font(.uiHeadline).foregroundStyle(Theme.ink)
            Text("We kept \(meeting.transcript.count) lines up to \(clock(stoppedAt)). Recording stopped there, and nothing after it was captured.")
                .foregroundStyle(Theme.ink)
            if preparing {
                HStack(spacing: 8) { ProgressView(); Text("writing up your notes…").foregroundStyle(Theme.muted) }
            } else {
                Button {
                    preparing = true
                    Task { @MainActor in
                        await model.finishRecovered(meeting.id)
                        preparing = false
                    }
                } label: { Text("finish with what we have").frame(maxWidth: .infinity) }
                    .buttonStyle(.brandPrimary)
                Button(role: .destructive) { confirmDiscard = true } label: { Text("discard").frame(maxWidth: .infinity) }
                    .buttonStyle(.brandQuiet)
            }
        }
        .controlSize(.large)
        .card()
    }

    // MARK: Transcript

    private func transcript(_ meeting: Meeting) -> some View {
        DisclosureGroup {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(meeting.transcript) { line in
                    HStack(alignment: .top, spacing: 10) {
                        Text(clock(line.time)).font(.brandMono).foregroundStyle(Theme.muted)
                        Text(line.text).foregroundStyle(Theme.ink)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, 8)
        } label: {
            Text("transcript").font(.brandHeading).foregroundStyle(Theme.ink)
        }
        .card()
    }

    private func sendNotes(_ meeting: Meeting, via service: ServiceKind) {
        let candidates = Contacts.candidates(for: nil, service: service)
        model.startAction(ActionRequest(
            service: service, content: meeting.notesText,
            recipient: candidates.count == 1 ? candidates[0] : nil, candidates: candidates
        ))
    }
}
