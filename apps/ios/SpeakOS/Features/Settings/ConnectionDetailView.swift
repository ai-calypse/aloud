import SwiftUI

/// Consent and status for one service. Used pushed from Settings (F09) and as a sheet when a
/// send needs a missing or expired connection, where it hands back to the preserved preview.
struct ConnectionDetailView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let service: ServiceKind
    let resume: ActionRequest?
    let isSheet: Bool

    @State private var connecting = false
    @State private var message: String?
    @State private var confirmDisconnect = false

    private var status: ConnectionStatus { model.connection(for: service).status }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Label(service.name, systemImage: service.symbol).font(.brandTitle).foregroundStyle(Theme.ink)
                Text(status.label).font(.brandMono).foregroundStyle(Theme.muted)
                if status == .connected, !model.connection(for: service).account.isEmpty {
                    Text("as \(model.connection(for: service).account)").foregroundStyle(Theme.ink)
                }

                if status != .connected {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("what speakos will be able to do").font(.uiHeadline).foregroundStyle(Theme.ink)
                        ForEach(service.access, id: \.self) { line in
                            Label(line, systemImage: "checkmark.circle").foregroundStyle(Theme.ink)
                        }
                        Text("Nothing goes out without you reviewing it first.").font(.uiFootnote).foregroundStyle(Theme.muted)
                    }
                    .card()
                }

                if let message {
                    Label(message, systemImage: "exclamationmark.triangle.fill")
                        .font(.uiSubheadline.weight(.semibold)).foregroundStyle(Theme.error)
                }

                if connecting {
                    HStack(spacing: 8) { ProgressView(); Text("handing off to \(service.name)…").foregroundStyle(Theme.muted) }
                } else if status == .connected {
                    Button("disconnect", role: .destructive) { confirmDisconnect = true }
                        .buttonStyle(.brandQuiet).controlSize(.large)
                } else {
                    Button { connect() } label: {
                        Text(status == .expired ? "reconnect \(service.name)" : "connect \(service.name)").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.brandPrimary).controlSize(.large)
                }
            }
            .padding(20)
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if isSheet {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { cancel() } }
            }
        }
        .interactiveDismissDisabled(connecting)
        .confirmationDialog("Disconnect \(service.name)?", isPresented: $confirmDisconnect, titleVisibility: .visible) {
            Button("Disconnect", role: .destructive) { model.setStatus(.notConnected, for: service) }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("speakos won't be able to use \(service.name) any more. Your saved notes stay.")
        }
    }

    private func connect() {
        connecting = true
        message = nil
        Task { @MainActor in
            let result = await model.connect(service)
            connecting = false
            switch result {
            case .connected:
                if let resume { model.present(.action(resume)) } else if isSheet { dismiss() }
            case .canceled:
                message = "connection canceled. nothing changed."
            case .failed:
                message = "couldn't connect. try again in a moment."
            }
        }
    }

    /// Cancelling a connection keeps the person's draft: back to the message, not to Home.
    private func cancel() {
        if let resume { model.present(.note(NoteDraft(text: resume.content, recipientName: resume.recipient?.name, existingID: nil))) } else { dismiss() }
    }
}
