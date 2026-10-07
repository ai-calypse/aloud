import SwiftUI

/// Every app SpeakOS can act in, grouped by what it is for. Connections run through Composio:
/// you sign in on the app's own page, and SpeakOS only learns that the link exists.
struct IntegrationsView: View {
    @Environment(AppModel.self) private var model
    @State private var query = ""
    @State private var category: IntegrationCategory?

    private var filtered: [Integration] {
        model.integrations.filter { item in
            (category == nil || item.category == category)
                && (query.isEmpty || (item.name + " " + item.summary).localizedCaseInsensitiveContains(query))
        }
    }

    var body: some View {
        let connected = filtered.filter { $0.status != .notConnected }
        let available = filtered.filter { $0.status == .notConnected }

        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                Text("integrations.").font(.brandTitle).foregroundStyle(Theme.ink).accessibilityAddTraits(.isHeader)

                HStack(alignment: .top, spacing: 12) {
                    Image(systemName: "lock.shield").font(.system(size: 20, weight: .semibold)).foregroundStyle(Theme.ink)
                        .frame(width: 40, height: 40).background(Theme.yellow, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                        .accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Connected through Composio").font(.brandCard).foregroundStyle(Theme.ink)
                        Text("You sign in on each app's own page. SpeakOS never sees your password, and nothing goes out until you review it.")
                            .font(.uiSubheadline).foregroundStyle(Theme.muted)
                    }
                }
                .card()

                SearchField(text: $query, prompt: "find an app…")

                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        chip("all", selected: category == nil) { category = nil }
                        ForEach(IntegrationCategory.allCases) { c in
                            chip(c.label.lowercased(), selected: category == c) { category = (category == c ? nil : c) }
                        }
                    }
                }

                if !connected.isEmpty { group("connected", connected) }
                if !available.isEmpty { group("available", available) }
                if filtered.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("not ringing a bell.").font(.brandHeading).foregroundStyle(Theme.ink)
                        Text("No app matches. Try a different word, or another category.").foregroundStyle(Theme.muted)
                    }
                }
            }
            .padding(.horizontal, 22).padding(.vertical, 12)
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
    }

    private func chip(_ title: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title).font(.uiSubheadline.weight(.semibold))
                .padding(.horizontal, 15).padding(.vertical, 9)
                .foregroundStyle(selected ? Color.white : Theme.ink)
                .background(selected ? Theme.ink : Theme.surface, in: Capsule())
                .overlay(Capsule().stroke(Theme.border))
        }
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private func group(_ title: String, _ items: [Integration]) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionTitle(text: title)
            VStack(spacing: 0) {
                ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                    NavigationLink { IntegrationDetailView(integrationID: item.id, resume: nil, isSheet: false) } label: { row(item) }
                        .buttonStyle(.plain)
                    if index < items.count - 1 { Divider().padding(.leading, 68) }
                }
            }
            .background(Theme.surface, in: RoundedRectangle(cornerRadius: Theme.cardRadius, style: .continuous))
            .shadow(color: Theme.ink.opacity(0.10), radius: 14, x: 0, y: 8)
        }
    }

    private func row(_ item: Integration) -> some View {
        HStack(spacing: 14) {
            Image(systemName: item.symbol).font(.system(size: 18, weight: .semibold)).foregroundStyle(Theme.ink)
                .frame(width: 40, height: 40).background(Theme.skySoft, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(item.name).font(.uiHeadline).foregroundStyle(Theme.ink)
                Text(item.status == .notConnected ? item.summary : (item.status == .expired ? "Needs reconnecting" : item.account))
                    .font(.uiSubheadline).foregroundStyle(item.status == .expired ? Theme.error : Theme.muted).lineLimit(1)
            }
            Spacer()
            Image(systemName: "chevron.right").font(.footnote.weight(.bold)).foregroundStyle(Theme.muted).accessibilityHidden(true)
        }
        .padding(14)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}

/// Consent, status and per-action permissions for one app. Pushed from Integrations, and shown as a
/// sheet when a send needs a missing or expired connection, where it hands back to the preserved preview.
struct IntegrationDetailView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let integrationID: String
    let resume: ActionRequest?
    let isSheet: Bool

    @State private var connecting = false
    @State private var message: String?
    @State private var confirmDisconnect = false

    var body: some View {
        if let item = model.integration(integrationID) {
            content(item)
        } else {
            ContentUnavailableView("not found.", systemImage: "questionmark.app")
        }
    }

    private func content(_ item: Integration) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                HStack(spacing: 14) {
                    Image(systemName: item.symbol).font(.system(size: 26, weight: .semibold)).foregroundStyle(Theme.ink)
                        .frame(width: 60, height: 60).background(Theme.skySoft, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                        .accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(item.name).font(.brandTitle).foregroundStyle(Theme.ink)
                        Text(item.status.label.uppercased()).font(.brandMono).tracking(0.8)
                            .foregroundStyle(item.status == .connected ? Theme.ink : (item.status == .expired ? Theme.error : Theme.muted))
                    }
                }
                Text(item.summary).font(.uiBody).foregroundStyle(Theme.muted)
                if item.status == .connected, !item.account.isEmpty {
                    Label("as \(item.account)", systemImage: "person.crop.circle").font(.uiSubheadline).foregroundStyle(Theme.ink)
                }

                if let message {
                    Label(message, systemImage: "exclamationmark.triangle.fill")
                        .font(.uiSubheadline.weight(.semibold)).foregroundStyle(Theme.error)
                }

                if connecting {
                    HStack(spacing: 8) { ProgressView(); Text("handing off to \(item.name)…").foregroundStyle(Theme.muted) }
                } else if item.status != .connected {
                    Button { connect(item) } label: {
                        Text(item.status == .expired ? "reconnect \(item.name)" : "connect \(item.name)")
                    }
                    .buttonStyle(.brandHot)
                    Text("Opens \(item.name)'s own sign-in page through Composio.").font(.uiFootnote).foregroundStyle(Theme.muted)
                }

                permissions(item)

                if item.status == .connected && !connecting {
                    Button("disconnect", role: .destructive) { confirmDisconnect = true }.buttonStyle(.brandQuiet)
                }
            }
            .padding(22)
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if isSheet { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { cancel() } } }
        }
        .interactiveDismissDisabled(connecting)
        .confirmationDialog("Disconnect \(item.name)?", isPresented: $confirmDisconnect, titleVisibility: .visible) {
            Button("Disconnect", role: .destructive) { model.setStatus(.notConnected, forID: item.id) }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("speakos won't be able to use \(item.name) any more. Your saved notes stay.")
        }
    }

    private func permissions(_ item: Integration) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionTitle(text: "what speakos can do")
            VStack(spacing: 0) {
                ForEach(Array(item.tools.enumerated()), id: \.element.id) { index, tool in
                    HStack(spacing: 12) {
                        Image(systemName: tool.writes ? "square.and.pencil" : "eye").foregroundStyle(Theme.muted).frame(width: 24).accessibilityHidden(true)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(tool.name).font(.uiHeadline).foregroundStyle(Theme.ink)
                            Text(tool.writes ? "Changes things. Always asks first." : (tool.asksFirst ? "Reads. Asks each time." : "Reads. Allowed without asking."))
                                .font(.uiFootnote).foregroundStyle(Theme.muted)
                        }
                        Spacer()
                        if !tool.writes {
                            Toggle("Ask each time", isOn: Binding(
                                get: { tool.asksFirst },
                                set: { model.setAsksFirst($0, tool: tool.name, in: item.id) }
                            ))
                            .labelsHidden()
                        }
                    }
                    .padding(14)
                    .accessibilityElement(children: .combine)
                    if index < item.tools.count - 1 { Divider().padding(.leading, 50) }
                }
            }
            .background(Theme.surface, in: RoundedRectangle(cornerRadius: Theme.cardRadius, style: .continuous))
            .shadow(color: Theme.ink.opacity(0.10), radius: 14, x: 0, y: 8)
        }
    }

    private func connect(_ item: Integration) {
        connecting = true
        message = nil
        Task { @MainActor in
            let result = await model.connect(item.id)
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

    /// Cancelling a connection keeps the person's words: back to the note, not to Home.
    private func cancel() {
        if let resume { model.present(.note(NoteDraft(text: resume.content, recipientName: resume.recipient?.name, existingID: nil))) } else { dismiss() }
    }
}
