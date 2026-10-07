import SwiftUI
import UIKit

/// Opens when you finish speaking (or edits a saved note). The words are yours to change by hand
/// or rewrite by tone and edit style. Nothing is saved or sent until you choose to.
struct NoteEditorSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let note: NoteDraft

    @State private var text: String
    /// The text before any rewrite. Each style rewrites from here, so styles never stack.
    @State private var base: String
    @State private var active: RewriteStyle?
    @State private var rewriting = false
    @State private var savedID: UUID?
    @State private var savedText: String?
    @State private var confirmClose = false

    init(note: NoteDraft) {
        self.note = note
        _text = State(initialValue: note.text)
        _base = State(initialValue: note.text)
        _savedID = State(initialValue: note.existingID)
        _savedText = State(initialValue: note.existingID == nil ? nil : note.text)
    }

    private var isBlank: Bool { text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    private var hasUnsaved: Bool { savedText != text && !isBlank }

    /// Typing replaces the rewrite baseline, so a style always applies to what you last wrote.
    private var editable: Binding<String> {
        Binding(get: { text }, set: { text = $0; base = $0; active = nil })
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Text(note.existingID == nil ? "your note." : "edit note.")
                        .font(.brandTitle).foregroundStyle(Theme.ink)
                        .accessibilityAddTraits(.isHeader)

                    TextEditor(text: editable)
                        .font(.uiBody)
                        .scrollContentBackground(.hidden)
                        .padding(14)
                        .frame(minHeight: 200)
                        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
                        .shadow(color: Theme.ink.opacity(0.10), radius: 14, x: 0, y: 8)
                        .overlay(alignment: .center) {
                            if rewriting {
                                RoundedRectangle(cornerRadius: 20, style: .continuous).fill(Theme.surface.opacity(0.85))
                                HStack(spacing: 8) { ProgressView(); Text("rewriting…").font(.uiSubheadline).foregroundStyle(Theme.muted) }
                            }
                        }
                        .accessibilityLabel("Note text")

                    rewriteSection

                    VStack(spacing: 12) {
                        InlineActions {
                            Button { UIPasteboard.general.string = text; model.showToast("copied.") } label: { Label("copy", systemImage: "doc.on.doc") }
                            ShareLink(item: text) { Label("share", systemImage: "square.and.arrow.up") }
                            Menu {
                                ForEach(ServiceKind.allCases) { service in
                                    Button { sendWith(service) } label: { Label(service.name, systemImage: service.symbol) }
                                }
                            } label: { Label("send with…", systemImage: "paperplane").frame(maxWidth: .infinity) }
                                .buttonStyle(.brandQuiet)
                        }
                        .disabled(isBlank)

                        Button { save(); dismiss() } label: { Text(note.existingID == nil ? "save to Memory" : "save changes").frame(maxWidth: .infinity) }
                            .buttonStyle(.brandHot)
                            .disabled(isBlank || rewriting)
                    }
                    Text("Copying or sharing doesn't save or send anything. Sending saves the note, then asks you to review first.")
                        .font(.uiFootnote).foregroundStyle(Theme.muted)
                }
                .padding(20)
            }
            .scrollDismissesKeyboard(.interactively)
            .screenBackground()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { if hasUnsaved { confirmClose = true } else { dismiss() } }
                }
            }
            .confirmationDialog("Keep this note?", isPresented: $confirmClose, titleVisibility: .visible) {
                Button("Save to Memory") { save(); dismiss() }
                Button("Discard", role: .destructive) { dismiss() }
                Button("Keep editing", role: .cancel) {}
            }
        }
        // Swiping away would skip that question, so it is off only while there are unsaved words.
        .interactiveDismissDisabled(hasUnsaved || rewriting)
    }

    // MARK: Rewrite

    private var rewriteSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                SectionTitle(text: "rewrite")
                Spacer()
                if active != nil {
                    Button("revert") { text = base; active = nil }
                        .font(.uiSubheadline.weight(.semibold)).foregroundStyle(Theme.primary)
                        .disabled(rewriting)
                }
            }
            chipRow("TONE", RewriteStyle.tones)
            chipRow("EDIT", RewriteStyle.edits)
        }
        .disabled(isBlank)
    }

    private func chipRow(_ title: String, _ styles: [RewriteStyle]) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).font(.brandMono).tracking(1).foregroundStyle(Theme.muted)
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(styles) { style in
                        Button { rewrite(style) } label: {
                            Text(style.label)
                                .font(.uiSubheadline.weight(.semibold))
                                .padding(.horizontal, 15).padding(.vertical, 9)
                                .foregroundStyle(active == style ? Color.white : Theme.ink)
                                .background(active == style ? Theme.ink : Theme.surface, in: Capsule())
                                .overlay(Capsule().stroke(Theme.border))
                        }
                        .accessibilityAddTraits(active == style ? .isSelected : [])
                    }
                }
            }
        }
        .disabled(rewriting)
    }

    private func rewrite(_ style: RewriteStyle) {
        rewriting = true
        let source = base
        Task { @MainActor in
            text = await model.rewrite(source, style: style)
            active = style
            rewriting = false
            AccessibilityNotification.Announcement("Rewritten: \(style.label)").post()
        }
    }

    // MARK: Save and send

    private func save() {
        savedID = model.saveNote(id: savedID, text: text)
        savedText = text
    }

    private func sendWith(_ service: ServiceKind) {
        // Sending replaces this sheet, so keep the words first rather than lose them.
        if hasUnsaved { save() }
        let candidates = Contacts.candidates(for: note.recipientName, service: service)
        model.startAction(ActionRequest(
            service: service, content: text,
            recipient: candidates.count == 1 ? candidates[0] : nil,
            candidates: candidates
        ))
    }
}
