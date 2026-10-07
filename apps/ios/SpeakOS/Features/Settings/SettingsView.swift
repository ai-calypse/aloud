import SwiftUI

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var confirmDelete = false
    @State private var firstUse = false

    var body: some View {
        @Bindable var model = model
        NavigationStack {
            Form {
                Section {
                    Toggle("bro mode", isOn: $model.broMode)
                    Toggle("haptics", isOn: $model.haptics)
                    Toggle("tap to record instead of hold", isOn: $model.tapToRecord)
                } header: {
                    Text("Little preferences")
                } footer: {
                    Text("Bro mode only adds warmth to headings and empty states. Recording status, confirmations and errors stay plain.")
                }

                Section("Connected apps") {
                    ForEach(model.connections) { connection in
                        NavigationLink {
                            ConnectionDetailView(service: connection.service, resume: nil, isSheet: false)
                        } label: {
                            HStack {
                                Label(connection.service.name, systemImage: connection.service.symbol)
                                Spacer()
                                Text(connection.status.label).font(.subheadline).foregroundStyle(Theme.muted)
                            }
                        }
                    }
                }

                Section("Your words") {
                    Text("Thoughts, drafts and notes are stored on this iPhone. Speech and the words you send to be understood are processed in the cloud, and only while you use a feature.")
                        .font(.subheadline).foregroundStyle(Theme.muted)
                    ShareLink(item: model.exportText.isEmpty ? "Nothing saved yet." : model.exportText) { Label("export your memory", systemImage: "square.and.arrow.up") }
                    Button("forget everything", role: .destructive) { confirmDelete = true }
                }

                Section("Help") {
                    DisclosureGroup("things you can say") {
                        VStack(alignment: .leading, spacing: 6) {
                            ForEach(DemoScript.samples, id: \.self) { Text("“\($0)”").font(.subheadline) }
                        }
                    }
                    DisclosureGroup("what gets saved") {
                        Text("Thoughts are saved on their own. Drafts only when you tap save draft. Finished meetings land in Meetings. A receipt appears after a connected action succeeds. Questions aren't saved.")
                            .font(.subheadline)
                    }
                }

                demoSection
            }
            .scrollContentBackground(.hidden)
            .screenBackground()
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
            .confirmationDialog("Forget everything?", isPresented: $confirmDelete, titleVisibility: .visible) {
                Button("Forget everything", role: .destructive) { model.deleteAll() }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This removes every thought, draft and meeting. It can't be undone. Connected apps stay connected.")
            }
        }
    }

    // The prototype's own controls: they make each state in the flows reachable without
    // a microphone, an account or a network. They would not exist in the shipped app.
    @ViewBuilder private var demoSection: some View {
        @Bindable var model = model
        Section {
            Toggle("first-use account (empty)", isOn: $firstUse)
                .onChange(of: firstUse) { model.applyFixtures(firstUse: firstUse) }
            Picker("microphone", selection: $model.micAccess) {
                ForEach(MicAccess.allCases) { Text($0.label).tag($0) }
            }
            Picker("next voice sample", selection: $model.demo.nextSample) {
                ForEach(DemoScript.sampleLabels.indices, id: \.self) { Text(DemoScript.sampleLabels[$0]).tag($0) }
            }
            Toggle("next capture fails", isOn: $model.demo.nextCaptureFails)
            Toggle("memory search unavailable", isOn: $model.demo.searchUnavailable)
            Picker("next send", selection: $model.demo.actionOutcome) {
                ForEach(ActionOutcome.allCases) { Text($0.label).tag($0) }
            }
            Picker("Slack", selection: Binding(
                get: { model.connection(for: .slack).status },
                set: { model.setStatus($0, for: .slack) }
            )) {
                ForEach(ConnectionStatus.allCases) { Text($0.label).tag($0) }
            }
            Picker("next connection attempt", selection: $model.demo.connectionResult) {
                ForEach(ConnectionResult.allCases) { Text($0.label).tag($0) }
            }
            Toggle("interrupt next meeting at 0:08", isOn: $model.demo.interruptMeeting)
            Toggle("next meeting notes fail", isOn: $model.demo.notesFail)
        } header: {
            Text("prototype controls")
        } footer: {
            Text("Simulated outcomes only. Nothing is recorded, sent or connected.")
        }
    }
}
