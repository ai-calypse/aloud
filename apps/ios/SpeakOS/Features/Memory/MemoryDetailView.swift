import SwiftUI

struct MemoryDetailView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let itemID: UUID

    @State private var isEditing = false
    @State private var title = ""
    @State private var text = ""
    @State private var confirmDiscard = false
    @State private var confirmForget = false

    var body: some View {
        Group {
            if let item = model.item(itemID) {
                content(item)
            } else {
                ContentUnavailableView("forgotten.", systemImage: "trash", description: Text("This one is gone."))
            }
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
    }

    @ViewBuilder private func content(_ item: MemoryItem) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    KindTag(kind: item.kind)
                    Spacer()
                    Text(item.date.formatted(date: .abbreviated, time: .shortened)).font(.brandMono).foregroundStyle(Theme.muted)
                }
                if isEditing {
                    TextField("Title", text: $title).font(.title2.weight(.bold)).textFieldStyle(.roundedBorder)
                    TextEditor(text: $text)
                        .frame(minHeight: 200)
                        .scrollContentBackground(.hidden)
                        .padding(10)
                        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(Theme.border))
                } else {
                    Text(item.title).font(.title2.weight(.bold)).foregroundStyle(Theme.ink)
                    Text(item.body).font(.body).foregroundStyle(Theme.ink)
                    if !item.tags.isEmpty {
                        Text(item.tags.map { "#\($0)" }.joined(separator: "  ")).font(.brandMono).foregroundStyle(Theme.muted)
                    }
                    // Forgetting is permanent, so it stays quiet until chosen, then asks once.
                    Button("forget this one", role: .destructive) { confirmForget = true }
                        .buttonStyle(.bordered).padding(.top, 12)
                }
            }
            .padding(20)
        }
        .navigationBarBackButtonHidden(isEditing)
        .toolbar {
            if isEditing {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { if title != item.title || text != item.body { confirmDiscard = true } else { isEditing = false } }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        model.updateItem(item.id, title: title, body: text)
                        isEditing = false
                    }.disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            } else {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    Button { model.togglePin(item.id) } label: {
                        Label(item.pinned ? "Unpin" : "Pin", systemImage: item.pinned ? "pin.slash" : "pin")
                    }
                    ShareLink(item: item.body)
                    Button("Edit") { title = item.title; text = item.body; isEditing = true }
                }
            }
        }
        .confirmationDialog("Discard your changes?", isPresented: $confirmDiscard, titleVisibility: .visible) {
            Button("Discard changes", role: .destructive) { isEditing = false }
            Button("Keep editing", role: .cancel) {}
        }
        .confirmationDialog("Forget this one for good?", isPresented: $confirmForget, titleVisibility: .visible) {
            Button("Forget it", role: .destructive) {
                model.deleteItem(item.id)
                dismiss()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("It can't be brought back.")
        }
    }
}
