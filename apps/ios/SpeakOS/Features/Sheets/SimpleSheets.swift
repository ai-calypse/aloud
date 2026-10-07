import SwiftUI

// Small sheets that share one shape: a short heading, one decision, a way out.

struct TypedInputSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var text: String
    @FocusState private var focused: Bool

    init(prefill: String) { _text = State(initialValue: prefill) }

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 16) {
                Text("same brain. quieter input.").font(.brandHeading).foregroundStyle(Theme.ink)
                TextEditor(text: $text)
                    .focused($focused)
                    .scrollContentBackground(.hidden)
                    .padding(10)
                    .frame(minHeight: 120)
                    .background(Theme.surface, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(Theme.border))
                    .accessibilityLabel("What do you want to say")
                Button {
                    let t = text
                    model.sheet = nil
                    model.submit(t)
                } label: { Text("go on, then →").frame(maxWidth: .infinity) }
                    .buttonStyle(.brandHot)
                    .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Spacer(minLength: 0)
            }
            .padding(20)
            .screenBackground()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .task { focused = true }
        }
    }
}

struct PermissionSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    let denied: Bool

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 16) {
                Image(systemName: denied ? "mic.slash" : "mic").font(.largeTitle).foregroundStyle(Theme.primary).accessibilityHidden(true)
                Text(denied ? "microphone access is off." : "speakos needs your microphone.")
                    .font(.brandHeading).foregroundStyle(Theme.ink)
                Text(denied
                     ? "type instead, or enable it in Settings."
                     : "It listens only while you hold the button, or during a meeting you start. Nothing is recorded in the background.")
                    .foregroundStyle(Theme.muted)
                if denied {
                    Button { if let url = URL(string: UIApplication.openSettingsURLString) { openURL(url) } } label: {
                        Text("open Settings").frame(maxWidth: .infinity)
                    }.buttonStyle(.brandPrimary)
                } else {
                    // Prototype: the real app calls the system permission prompt here.
                    Button {
                        model.micAccess = .granted
                        dismiss()
                    } label: { Text("allow microphone").frame(maxWidth: .infinity) }
                        .buttonStyle(.brandPrimary)
                }
                Button { model.present(.typed(prefill: "")) } label: { Text("type instead").frame(maxWidth: .infinity) }
                    .buttonStyle(.brandQuiet)
                Spacer(minLength: 0)
            }
            .controlSize(.large)
            .padding(20)
            .screenBackground()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Not now") { dismiss() } } }
        }
    }
}

/// One short question about the one thing we could not tell, never a classification quiz.
struct ClarifySheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let clarification: Clarification

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 16) {
                Text("save this thought or make it a message?").font(.brandHeading).foregroundStyle(Theme.ink)
                Text(clarification.text).foregroundStyle(Theme.ink)
                    .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                    .background(Theme.surface, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Theme.border))
                Button {
                    dismiss()
                    model.saveThought(Interpreter.sentence(clarification.text))
                } label: { Text("save as a thought").frame(maxWidth: .infinity) }
                    .buttonStyle(.brandPrimary)
                Button {
                    model.present(.draft(DraftModel(text: Interpreter.sentence(clarification.text), recipientName: nil)))
                } label: { Text("make it a message").frame(maxWidth: .infinity) }
                    .buttonStyle(.brandQuiet)
                Spacer(minLength: 0)
            }
            .controlSize(.large)
            .padding(20)
            .screenBackground()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
        }
    }
}

/// Several people match, so the person picks. We never show a final approval with a guessed recipient.
struct RecipientSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let request: ActionRequest

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ForEach(request.candidates) { person in
                        Button { model.choose(person, for: request) } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(person.name).font(.uiHeadline).foregroundStyle(Theme.ink)
                                Text("\(request.service.name) · \(person.detail)").font(.uiSubheadline).foregroundStyle(Theme.muted)
                            }
                        }
                    }
                } header: {
                    Text("which one?").font(.brandHeading).foregroundStyle(Theme.ink).textCase(nil)
                } footer: {
                    Text("Nothing is sent until you review it.")
                }
            }
            .scrollContentBackground(.hidden)
            .screenBackground()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
        }
    }
}

struct ThoughtEditorSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let itemID: UUID
    @State private var text = ""
    @State private var original = ""
    @State private var confirmDiscard = false

    private var dirty: Bool { text != original }

    var body: some View {
        NavigationStack {
            TextEditor(text: $text)
                .scrollContentBackground(.hidden)
                .padding(12)
                .background(Theme.surface, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                .padding(20)
                .screenBackground()
                .navigationTitle("Edit thought")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Cancel") { if dirty { confirmDiscard = true } else { dismiss() } }
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Save") {
                            model.updateItem(itemID, title: String(text.prefix(60)), body: text)
                            dismiss()
                        }.disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    }
                }
                .confirmationDialog("Discard your changes?", isPresented: $confirmDiscard, titleVisibility: .visible) {
                    Button("Discard changes", role: .destructive) { dismiss() }
                    Button("Keep editing", role: .cancel) {}
                }
        }
        .interactiveDismissDisabled(dirty)
        .onAppear {
            let body = model.item(itemID)?.body ?? ""
            text = body
            original = body
        }
    }
}
