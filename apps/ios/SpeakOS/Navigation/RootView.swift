import SwiftUI

enum AppTab: String, CaseIterable, Identifiable, Hashable {
    case memory, speak, meetings

    var id: String { rawValue }
    var title: String {
        switch self {
        case .memory: "Memory"
        case .speak: "Speak"
        case .meetings: "Meetings"
        }
    }
    var symbol: String {
        switch self {
        case .memory: "tray.full"
        case .speak: "mic"
        case .meetings: "person.2"
        }
    }

    @ViewBuilder var content: some View {
        switch self {
        case .memory: MemoryView()
        case .speak: SpeakView()
        case .meetings: MeetingsView()
        }
    }
}

/// Every modal in the app. Presenting is always `model.present(_:)`, so only one is up at a time.
enum AppSheet: Identifiable {
    case typed(prefill: String)
    case permission(denied: Bool)
    case recipient(ActionRequest)
    case note(NoteDraft)
    case action(ActionRequest)
    case answer(AnswerModel)
    case connect(ServiceKind, resume: ActionRequest?)
    case settings

    var id: String {
        switch self {
        case .typed: "typed"
        case .permission: "permission"
        case .recipient(let r): "recipient-\(r.id)"
        case .note(let n): "note-\(n.id)"
        case .action(let r): "action-\(r.id)"
        case .answer(let a): "answer-\(a.id)"
        case .connect(let s, _): "connect-\(s.id)"
        case .settings: "settings"
        }
    }
}

struct RootView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        @Bindable var model = model
        TabView(selection: $model.selectedTab) {
            ForEach(AppTab.allCases) { tab in
                tab.content
                    .tabItem { Label(tab.title, systemImage: tab.symbol) }
                    .tag(tab)
            }
        }
        .overlay(alignment: .top) {
            if let text = model.toastText {
                ToastView(text: text)
                    .transition(reduceMotion ? .opacity : .move(edge: .top).combined(with: .opacity))
                    .allowsHitTesting(false)
            }
        }
        .animation(.easeInOut(duration: 0.2), value: model.toastText)
        .sheet(item: $model.sheet) { sheet in
            SheetContent(sheet: sheet)
        }
        .fullScreenCover(isPresented: Binding(get: { model.session != nil }, set: { _ in })) {
            MeetingSessionView()
        }
    }
}

private struct SheetContent: View {
    let sheet: AppSheet

    var body: some View {
        switch sheet {
        case .typed(let prefill):
            TypedInputSheet(prefill: prefill)
                .presentationDetents([.medium, .large])
                .presentationDragIndicator(.visible)
        case .permission(let denied):
            PermissionSheet(denied: denied)
                .presentationDetents([.medium])
                .presentationDragIndicator(.visible)
        case .recipient(let r):
            RecipientSheet(request: r)
                .presentationDetents([.medium, .large])
                .presentationDragIndicator(.visible)
        case .note(let n):
            NoteEditorSheet(note: n)
                .presentationDetents([.large])
        case .action(let r):
            ActionPreviewSheet(request: r)
                .presentationDetents([.large])
        case .answer(let a):
            AnswerSheet(answer: a)
                .presentationDetents([.large])
        case .connect(let s, let resume):
            NavigationStack { ConnectionDetailView(service: s, resume: resume, isSheet: true) }
                .presentationDetents([.medium, .large])
                .presentationDragIndicator(.visible)
        case .settings:
            SettingsView()
        }
    }
}

struct RouteDestination: View {
    let route: Route
    var body: some View {
        switch route {
        case .memory(let id): MemoryDetailView(itemID: id)
        case .meeting(let id): MeetingDetailView(meetingID: id)
        }
    }
}

struct SettingsToolbar: ToolbarContent {
    @Environment(AppModel.self) private var model
    var body: some ToolbarContent {
        ToolbarItem(placement: .topBarTrailing) {
            Button { model.present(.settings) } label: { Label("Settings", systemImage: "gearshape") }
        }
    }
}
