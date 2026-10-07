import SwiftUI

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var confirmDelete = false
    @State private var showAccount = false
    @State private var firstUse = false

    var body: some View {
        @Bindable var model = model
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 26) {
                    Text("settings.").font(.brandTitle).foregroundStyle(Theme.ink).accessibilityAddTraits(.isHeader)

                    accountCard
                    capture
                    rewriting
                    organise
                    privacy
                    help
                    demoSection
                    Text("SpeakOS 0.1.0 (1) · prototype").font(.brandMono).foregroundStyle(Theme.muted)
                        .frame(maxWidth: .infinity)
                }
                .padding(.horizontal, 22).padding(.vertical, 12)
            }
            .screenBackground()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
            .sheet(isPresented: $showAccount) { AccountSheet().presentationDetents([.medium]).presentationDragIndicator(.visible) }
            .confirmationDialog("Forget everything?", isPresented: $confirmDelete, titleVisibility: .visible) {
                Button("Forget everything", role: .destructive) { model.deleteAll() }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This removes every note and meeting. It can't be undone. Connected apps stay connected.")
            }
        }
    }

    // MARK: Sections

    @ViewBuilder private var accountCard: some View {
        if let email = model.account {
            group("Account") {
                row("person.crop.circle", "Signed in", detail: email)
                action("rectangle.portrait.and.arrow.right", "Sign out", destructive: true) { model.account = nil; model.showToast("signed out.") }
            }
        } else {
            VStack(spacing: 12) {
                Text("Keep your notes everywhere").font(.brandHeading).foregroundStyle(Theme.ink)
                Text("Sign up or log in to keep your notes synced across all your devices.")
                    .font(.uiSubheadline).foregroundStyle(Theme.muted).multilineTextAlignment(.center)
                Button { showAccount = true } label: { Text("sign up") }.buttonStyle(.brandHot)
                Button("or log in") { showAccount = true }
                    .font(.uiSubheadline.weight(.semibold)).foregroundStyle(Theme.primary)
            }
            .frame(maxWidth: .infinity)
            .card(padding: 20)
        }
    }

    private var capture: some View {
        @Bindable var model = model
        return group("Capture") {
            toggleRow("hand.tap", "Tap to record", "Instead of holding the sky window, tap to start and stop.", $model.tapToRecord)
            toggleRow("iphone.radiowaves.left.and.right", "Haptics", nil, $model.haptics)
            toggleRow("waveform", "Keep audio", "Save the recording with each note. Off by default.", $model.keepAudio)
            pickerRow("globe", "Speech language", selection: $model.language) {
                ForEach(SpeechLanguage.allCases) { Text($0.rawValue).tag($0) }
            }
        }
    }

    private var rewriting: some View {
        @Bindable var model = model
        return group("Rewriting") {
            pickerRow("wand.and.stars", "Default tone", selection: $model.defaultTone) {
                Text("None").tag(RewriteStyle?.none)
                ForEach(RewriteStyle.tones) { Text($0.label).tag(Optional($0)) }
            }
            link("text.badge.checkmark", "Text replacements", detail: "\(model.replacements.count)") { TextReplacementsView() }
        }
    }

    private var organise: some View {
        group("Organise") {
            link("number", "Tags", detail: "\(model.allTags.count)") { TagsView() }
            link("square.grid.2x2", "Integrations", detail: "\(model.connectedCount) connected") { IntegrationsView() }
        }
    }

    private var privacy: some View {
        group("Your words") {
            Text("Notes are stored on this iPhone. Speech and the words you send to be understood are processed in the cloud, and only while you use a feature.")
                .font(.uiSubheadline).foregroundStyle(Theme.muted).padding(14)
            ShareLink(item: model.exportText.isEmpty ? "Nothing saved yet." : model.exportText) {
                rowLabel("square.and.arrow.up", "Export data", detail: nil, chevron: true)
            }
            action("trash", "Forget everything", destructive: true) { confirmDelete = true }
        }
    }

    private var help: some View {
        group("Help & feedback") {
            ShareLink(item: "Try SpeakOS: say it, keep the note.") { rowLabel("heart", "Share the app", detail: nil, chevron: true) }
            action("bubble.left", "Give feedback") { model.showToast("opens your mail app.") }
            action("lightbulb", "Suggest a feature") { model.showToast("opens in your browser.") }
            DisclosureGroup {
                Text("After you speak, your words open as a note. It's saved only when you tap save to Memory. Meetings are kept separately in Meetings. A receipt appears after a connected action succeeds. Questions aren't saved.")
                    .font(.uiSubheadline).foregroundStyle(Theme.muted).padding(.top, 6)
            } label: { Label("What gets saved", systemImage: "questionmark.circle").font(.uiHeadline).foregroundStyle(Theme.ink) }
                .padding(14)
            action("hand.raised", "Privacy policy") { model.showToast("opens in your browser.") }
            action("doc.plaintext", "Terms of use") { model.showToast("opens in your browser.") }
        }
    }

    // The prototype's own controls: they make each state in the flows reachable without a
    // microphone, an account or a network. They would not exist in the shipped app.
    private var demoSection: some View {
        @Bindable var model = model
        return group("Prototype controls", footer: "Simulated outcomes only. Nothing is recorded, sent or connected.") {
            toggleRow("person.badge.plus", "First-use account (empty)", nil, $firstUse)
                .onChange(of: firstUse) { model.applyFixtures(firstUse: firstUse) }
            pickerRow("mic", "Microphone", selection: $model.micAccess) { ForEach(MicAccess.allCases) { Text($0.label).tag($0) } }
            pickerRow("quote.bubble", "Next voice sample", selection: $model.demo.nextSample) {
                ForEach(DemoScript.sampleLabels.indices, id: \.self) { Text(DemoScript.sampleLabels[$0]).tag($0) }
            }
            toggleRow("exclamationmark.triangle", "Next capture fails", nil, $model.demo.nextCaptureFails)
            toggleRow("magnifyingglass", "Search unavailable", nil, $model.demo.searchUnavailable)
            pickerRow("paperplane", "Next send", selection: $model.demo.actionOutcome) { ForEach(ActionOutcome.allCases) { Text($0.label).tag($0) } }
            pickerRow("number.square", "Slack", selection: Binding(
                get: { model.connection(for: .slack).status },
                set: { model.setStatus($0, for: .slack) }
            )) { ForEach(ConnectionStatus.allCases) { Text($0.label).tag($0) } }
            pickerRow("link", "Next connection", selection: $model.demo.connectionResult) { ForEach(ConnectionResult.allCases) { Text($0.label).tag($0) } }
            toggleRow("person.2.slash", "Interrupt next meeting at 0:08", nil, $model.demo.interruptMeeting)
            toggleRow("doc.badge.ellipsis", "Next meeting notes fail", nil, $model.demo.notesFail)
        }
    }

    // MARK: Building blocks

    private func group<Content: View>(_ title: String, footer: String? = nil, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title.uppercased()).font(.brandMono).tracking(1).foregroundStyle(Theme.muted).padding(.leading, 4)
                .accessibilityAddTraits(.isHeader)
            VStack(spacing: 0) { content() }
                .background(Theme.surface, in: RoundedRectangle(cornerRadius: Theme.cardRadius, style: .continuous))
                .shadow(color: Theme.ink.opacity(0.10), radius: 14, x: 0, y: 8)
            if let footer { Text(footer).font(.uiFootnote).foregroundStyle(Theme.muted).padding(.leading, 4) }
        }
    }

    private func iconChip(_ symbol: String) -> some View {
        Image(systemName: symbol).font(.system(size: 15, weight: .semibold)).foregroundStyle(Theme.ink)
            .frame(width: 32, height: 32).background(Theme.skySoft, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            .accessibilityHidden(true)
    }

    private func rowLabel(_ symbol: String, _ title: String, detail: String?, chevron: Bool, destructive: Bool = false) -> some View {
        HStack(spacing: 12) {
            iconChip(symbol)
            Text(title).font(.uiHeadline).foregroundStyle(destructive ? Theme.error : Theme.ink)
            Spacer()
            if let detail { Text(detail).font(.uiSubheadline).foregroundStyle(Theme.muted) }
            if chevron { Image(systemName: "chevron.right").font(.footnote.weight(.bold)).foregroundStyle(Theme.muted).accessibilityHidden(true) }
        }
        .padding(14)
        .contentShape(Rectangle())
    }

    private func row(_ symbol: String, _ title: String, detail: String) -> some View {
        rowLabel(symbol, title, detail: detail, chevron: false).accessibilityElement(children: .combine)
    }

    private func action(_ symbol: String, _ title: String, destructive: Bool = false, perform: @escaping () -> Void) -> some View {
        Button(action: perform) { rowLabel(symbol, title, detail: nil, chevron: !destructive, destructive: destructive) }
            .buttonStyle(.plain)
    }

    private func link<Destination: View>(_ symbol: String, _ title: String, detail: String, @ViewBuilder destination: () -> Destination) -> some View {
        NavigationLink { destination() } label: { rowLabel(symbol, title, detail: detail, chevron: true) }
            .buttonStyle(.plain)
    }

    private func toggleRow(_ symbol: String, _ title: String, _ subtitle: String?, _ isOn: Binding<Bool>) -> some View {
        Toggle(isOn: isOn) {
            HStack(spacing: 12) {
                iconChip(symbol)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.uiHeadline).foregroundStyle(Theme.ink)
                    if let subtitle { Text(subtitle).font(.uiFootnote).foregroundStyle(Theme.muted) }
                }
            }
        }
        .padding(14)
    }

    private func pickerRow<Value: Hashable, Options: View>(_ symbol: String, _ title: String, selection: Binding<Value>, @ViewBuilder options: () -> Options) -> some View {
        HStack(spacing: 12) {
            iconChip(symbol)
            Text(title).font(.uiHeadline).foregroundStyle(Theme.ink)
            Spacer()
            Picker(title, selection: selection) { options() }
                .labelsHidden().tint(Theme.muted)
        }
        .padding(.horizontal, 14).padding(.vertical, 8)
    }
}

struct AccountSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var email = ""

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 16) {
                Text("sync your notes.").font(.brandTitle).foregroundStyle(Theme.ink)
                Text("We'll email you a sign-in link. No password to remember.").foregroundStyle(Theme.muted)
                TextField("you@example.com", text: $email)
                    .keyboardType(.emailAddress).textContentType(.emailAddress)
                    .textInputAutocapitalization(.never).autocorrectionDisabled()
                    .padding(14).background(Theme.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(Theme.border))
                // Prototype: the real app sends a link; here the address is simply accepted.
                Button {
                    model.account = email.trimmingCharacters(in: .whitespaces)
                    dismiss()
                } label: { Text("continue") }
                    .buttonStyle(.brandHot)
                    .disabled(!email.contains("@"))
                Spacer(minLength: 0)
            }
            .padding(22)
            .screenBackground()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
        }
    }
}

/// Shortcuts for words you say often ("my email" becomes an address). Applied to new dictation only.
struct TextReplacementsView: View {
    @Environment(AppModel.self) private var model
    @State private var from = ""
    @State private var to = ""

    var body: some View {
        @Bindable var model = model
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("text replacements.").font(.brandTitle).foregroundStyle(Theme.ink).accessibilityAddTraits(.isHeader)
                Text("Say the short phrase and SpeakOS writes the long one. Applies to new dictation, not saved notes.")
                    .foregroundStyle(Theme.muted)

                VStack(spacing: 10) {
                    TextField("when I say…", text: $from).textInputAutocapitalization(.never)
                    TextField("write…", text: $to).textInputAutocapitalization(.never)
                    Button { add() } label: { Text("add replacement") }
                        .buttonStyle(.brandPrimary)
                        .disabled(from.trimmingCharacters(in: .whitespaces).isEmpty || to.isEmpty)
                }
                .textFieldStyle(.roundedBorder)
                .card()

                if !model.replacements.isEmpty {
                    VStack(spacing: 0) {
                        ForEach(Array(model.replacements.enumerated()), id: \.element.id) { index, item in
                            HStack(spacing: 10) {
                                Text(item.from).font(.uiHeadline).foregroundStyle(Theme.ink)
                                Image(systemName: "arrow.right").font(.footnote).foregroundStyle(Theme.muted).accessibilityLabel("becomes")
                                Text(item.to).font(.uiBody).foregroundStyle(Theme.muted).lineLimit(1)
                                Spacer()
                                Button { model.replacements.removeAll { $0.id == item.id } } label: {
                                    Image(systemName: "trash").foregroundStyle(Theme.error).frame(width: 36, height: 36)
                                }
                                .accessibilityLabel("Delete replacement \(item.from)")
                            }
                            .padding(.horizontal, 16).padding(.vertical, 6)
                            if index < model.replacements.count - 1 { Divider().padding(.leading, 16) }
                        }
                    }
                    .background(Theme.surface, in: RoundedRectangle(cornerRadius: Theme.cardRadius, style: .continuous))
                    .shadow(color: Theme.ink.opacity(0.10), radius: 14, x: 0, y: 8)
                }
            }
            .padding(22)
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
    }

    private func add() {
        model.replacements.append(TextReplacement(from: from.trimmingCharacters(in: .whitespaces), to: to))
        from = ""
        to = ""
    }
}
