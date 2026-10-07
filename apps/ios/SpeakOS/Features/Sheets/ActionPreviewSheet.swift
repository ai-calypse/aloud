import SwiftUI

/// The approval screen for a connected action. The exact destination and content stay on
/// screen from review through the outcome, and an unknown outcome is never shown as a failure
/// (which invites a duplicate send) or as success.
struct ActionPreviewSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let request: ActionRequest
    @State private var content: String
    @State private var stage: Stage = .review

    enum Stage { case review, pending, sent, failed, unknown }

    init(request: ActionRequest) {
        self.request = request
        _content = State(initialValue: request.content)
    }

    private var service: ServiceKind { request.service }
    private var working: Bool { stage == .pending || stage == .unknown }

    private var current: ActionRequest {
        var r = request
        r.content = content
        return r
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    heading
                    destination
                    contentCard
                    status
                    buttons
                }
                .padding(20)
            }
            .scrollDismissesKeyboard(.interactively)
            .screenBackground()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(stage == .sent ? "Close" : "Cancel") { dismiss() }.disabled(working)
                }
            }
        }
        .interactiveDismissDisabled(working)
    }

    private var heading: some View {
        HStack(spacing: 10) {
            if stage == .sent { PixelSpark(color: Theme.orange) }
            Text(stage == .sent ? service.doneLine + "." : service.readyLine)
                .font(.brandHeading).foregroundStyle(Theme.ink)
        }
        .accessibilityAddTraits(.isHeader)
    }

    private var destination: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(service.name, systemImage: service.symbol).font(.brandMono).foregroundStyle(Theme.primary)
            Text("\(service.destinationLabel) \(request.recipient?.name ?? "—")").font(.headline).foregroundStyle(Theme.ink)
            if let detail = request.recipient?.detail { Text(detail).font(.subheadline).foregroundStyle(Theme.muted) }
            let account = model.connection(for: service).account
            if !account.isEmpty { Text("from \(account)").font(.footnote).foregroundStyle(Theme.muted) }
        }
        .card()
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder private var contentCard: some View {
        if stage == .review || stage == .failed {
            TextEditor(text: $content)
                .scrollContentBackground(.hidden)
                .padding(10)
                .frame(minHeight: 140)
                .background(Theme.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(Theme.border))
                .accessibilityLabel("Message content")
        } else {
            Text(content).foregroundStyle(Theme.ink).card()
        }
    }

    @ViewBuilder private var status: some View {
        switch stage {
        case .review: EmptyView()
        case .pending:
            HStack(spacing: 8) { ProgressView(); Text(service == .notion ? "creating…" : "sending…").foregroundStyle(Theme.muted) }
        case .sent:
            Text("\(service.doneLine) · \(ago(.now))").font(.brandMono).foregroundStyle(Theme.muted)
        case .failed:
            Label("couldn't \(service.verb). review it and try again.", systemImage: "exclamationmark.triangle.fill")
                .font(.subheadline.weight(.semibold)).foregroundStyle(Theme.error)
        case .unknown:
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) { ProgressView(); Text("checking whether it went through…").foregroundStyle(Theme.ink) }
                Text("Please don't \(service.verb) it again. It may already have gone.").font(.footnote).foregroundStyle(Theme.muted)
            }
        }
    }

    @ViewBuilder private var buttons: some View {
        switch stage {
        case .review, .failed:
            Button { send() } label: { Text(stage == .failed ? "try again" : service.verb).frame(maxWidth: .infinity) }
                .buttonStyle(.borderedProminent).controlSize(.large)
                .disabled(content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            Button("cancel", role: .cancel) { dismiss() }.buttonStyle(.bordered).controlSize(.large)
        case .pending:
            EmptyView()
        case .unknown:
            Button { check() } label: { Text("check again").frame(maxWidth: .infinity) }
                .buttonStyle(.bordered).controlSize(.large)
        case .sent:
            Button { dismiss() } label: { Text("done").frame(maxWidth: .infinity) }
                .buttonStyle(.borderedProminent).controlSize(.large)
        }
    }

    private func send() {
        stage = .pending
        let r = current
        Task { @MainActor in
            switch await model.performSend(r) {
            case .sent: stage = .sent
            case .failed: stage = .failed
            case .unknown: stage = .unknown
            }
        }
    }

    private func check() {
        let r = current
        Task { @MainActor in
            stage = await model.checkStatus(r) == .sent ? .sent : .unknown
        }
    }
}
