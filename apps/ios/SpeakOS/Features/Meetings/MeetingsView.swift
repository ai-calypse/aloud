import SwiftUI

struct MeetingsView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        NavigationStack(path: $model.meetingsPath) {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text(model.warm("be there. we'll take notes.", bro: "be there, bro. we'll take notes."))
                        .font(.brandTitle).foregroundStyle(Theme.ink)
                        .accessibilityAddTraits(.isHeader)
                    SearchField(text: $model.query, prompt: "search meetings…")
                    Button { model.startMeeting() } label: {
                        Label("start a meeting", systemImage: "record.circle").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.brandHot)

                    if !model.query.isEmpty && model.visibleMeetings.isEmpty {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("not ringing a bell.").font(.brandHeading).foregroundStyle(Theme.ink)
                            Text("No meeting matches. Try a different word.").foregroundStyle(Theme.muted)
                        }
                    } else if model.meetings.isEmpty {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(model.warm("no meetings. lucky you.", bro: "no meetings. lucky you, bro.")).font(.uiHeadline).foregroundStyle(Theme.ink)
                            Text("your notes will live here after a session.").font(.uiSubheadline).foregroundStyle(Theme.muted)
                        }
                        .card()
                        .accessibilityElement(children: .combine)
                    } else {
                        ForEach(model.visibleMeetings) { meeting in
                            NavigationLink(value: Route.meeting(meeting.id)) { MeetingRow(meeting: meeting) }
                                .buttonStyle(.plain)
                        }
                    }
                }
                .padding(.horizontal, 20)
                .padding(.vertical, 12)
            }
            .screenBackground()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { Wordmark() }
                SettingsToolbar()
            }
            .navigationDestination(for: Route.self) { RouteDestination(route: $0) }
        }
    }
}

private struct MeetingRow: View {
    let meeting: Meeting

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Label("meeting", systemImage: "person.2").font(.brandMono).foregroundStyle(Theme.primary)
                Spacer()
                Text(ago(meeting.date)).font(.brandMono).foregroundStyle(Theme.muted)
            }
            Text(meeting.title).font(.uiHeadline).foregroundStyle(Theme.ink)
            switch meeting.status {
            case .finished:
                if !meeting.summary.isEmpty {
                    Text(meeting.summary).font(.uiSubheadline).foregroundStyle(Theme.muted).lineLimit(2).multilineTextAlignment(.leading)
                }
                Text("\(meeting.decisions.count) decisions · \(meeting.actionItems.count) action items · \(clock(meeting.duration))")
                    .font(.brandMono).foregroundStyle(Theme.muted)
            case .unfinished(let at):
                Label("unfinished. recording stopped at \(clock(at)).", systemImage: "exclamationmark.circle")
                    .font(.uiSubheadline.weight(.semibold)).foregroundStyle(Theme.ink)
            }
        }
        .card()
        .accessibilityElement(children: .combine)
    }
}
