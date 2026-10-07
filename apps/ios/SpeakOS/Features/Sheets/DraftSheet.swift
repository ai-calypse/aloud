import SwiftUI
import UIKit

/// "your words, ready." Editable text with copy, share and save. Nothing here sends anything:
/// "send with…" opens the reviewed action preview instead.
struct DraftSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let draft: DraftModel
    @State private var text: String
    @State private var savedID: UUID?
    @State private var copied = false
    @State private var confirmClose = false

    init(draft: DraftModel) {
        self.draft = draft
        _text = State(initialValue: draft.text)
    }

    /// Only an edited, unsaved draft needs a keep/discard question.
    private var hasUnsavedEdits: Bool { savedID == nil && text != draft.text }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text("your words, ready.").font(.brandHeading).foregroundStyle(Theme.ink)
                    TextEditor(text: $text)
                        .scrollContentBackground(.hidden)
                        .padding(10)
                        .frame(minHeight: 180)
                        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(Theme.border))
                        .accessibilityLabel("Draft text")

                    InlineActions {
                        Button {
                            UIPasteboard.general.string = text
                            copied = true
                            model.showToast("copied.")
                        } label: { Label(copied ? "copied" : "copy", systemImage: copied ? "checkmark" : "doc.on.doc") }
                        ShareLink(item: text) { Label("share", systemImage: "square.and.arrow.up") }
                        Button {
                            savedID = model.saveDraft(text)
                        } label: { Label(savedID == nil ? "save draft" : "saved", systemImage: savedID == nil ? "tray.and.arrow.down" : "checkmark") }
                            .disabled(savedID != nil)
                    }

                    Menu {
                        ForEach([ServiceKind.slack, .gmail], id: \.self) { service in
                            Button { sendWith(service) } label: { Label(service.name, systemImage: service.symbol) }
                        }
                    } label: { Label("send with…", systemImage: "paperplane").frame(maxWidth: .infinity) }
                        .buttonStyle(.brandPrimary).controlSize(.large)
                        .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)

                    Text("Sharing or copying doesn't send anything. Sending asks you to review first.")
                        .font(.uiFootnote).foregroundStyle(Theme.muted)
                }
                .padding(20)
            }
            .scrollDismissesKeyboard(.interactively)
            .screenBackground()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { if hasUnsavedEdits { confirmClose = true } else { dismiss() } }
                }
            }
            .confirmationDialog("You changed this draft.", isPresented: $confirmClose, titleVisibility: .visible) {
                Button("Save draft") { model.saveDraft(text); dismiss() }
                Button("Discard", role: .destructive) { dismiss() }
                Button("Keep editing", role: .cancel) {}
            }
        }
        // Swiping away would skip the question, so it is off only while edits are unsaved.
        .interactiveDismissDisabled(hasUnsavedEdits)
    }

    private func sendWith(_ service: ServiceKind) {
        let candidates = Contacts.candidates(for: draft.recipientName, service: service)
        let request = ActionRequest(
            service: service, content: text,
            recipient: candidates.count == 1 ? candidates[0] : nil,
            candidates: candidates
        )
        model.startAction(request)
    }
}
