import SwiftUI

struct MemoryDetailView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let itemID: UUID

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
                Text(item.title).font(.brandHeading).foregroundStyle(Theme.ink)
                Text(item.body).font(.uiBody).foregroundStyle(Theme.ink)
                if !item.tags.isEmpty {
                    Text(item.tags.map { "#\($0)" }.joined(separator: "  ")).font(.brandMono).foregroundStyle(Theme.muted)
                }
                if item.kind == .note {
                    // Same editor as after dictation, so a saved note can be rewritten too.
                    Button { model.present(.note(NoteDraft(text: item.body, recipientName: nil, existingID: item.id))) } label: {
                        Label("edit and rewrite", systemImage: "wand.and.stars").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.brandHot)
                }
                // Forgetting is permanent, so it stays quiet until chosen, then asks once.
                Button("forget this one", role: .destructive) { confirmForget = true }
                    .buttonStyle(.brandQuiet).padding(.top, 8)
            }
            .padding(20)
        }
        .toolbar {
            ToolbarItemGroup(placement: .topBarTrailing) {
                Button { model.togglePin(item.id) } label: {
                    Label(item.pinned ? "Unpin" : "Pin", systemImage: item.pinned ? "pin.slash" : "pin")
                }
                ShareLink(item: item.body)
            }
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
