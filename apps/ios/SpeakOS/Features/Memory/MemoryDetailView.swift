import SwiftUI

/// The page a note opens on: a calm reading surface, with rewriting and sending one tap away
/// in a floating bar. The same page shows sent receipts and meeting summaries, without the editor.
struct MemoryDetailView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let itemID: UUID

    @State private var confirmForget = false

    private let leadFont = Font.custom("Geist", size: 20, relativeTo: .title3)
    private let titleFont = Font.custom("Bricolage Grotesque", size: 32, relativeTo: .title).weight(.heavy)

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

    private func words(_ text: String) -> Int { text.split { $0.isWhitespace }.count }

    @ViewBuilder private func content(_ item: MemoryItem) -> some View {
        // Notes get their title from their first words, so repeating it above the body would read twice.
        let titleIsDerived = item.body.hasPrefix(item.title)
        let count = words(item.body)

        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                HStack {
                    KindTag(kind: item.kind)
                    if item.pinned { Image(systemName: "pin.fill").font(.caption).foregroundStyle(Theme.muted).accessibilityLabel("pinned") }
                    Spacer()
                    Text(item.date.formatted(date: .abbreviated, time: .shortened)).font(.brandMono).foregroundStyle(Theme.muted)
                }

                if !titleIsDerived {
                    Text(item.title).font(titleFont).foregroundStyle(Theme.ink)
                        .accessibilityAddTraits(.isHeader)
                }

                Text(item.body)
                    .font(titleIsDerived ? Font.custom("Bricolage Grotesque", size: 26, relativeTo: .title2).weight(.bold) : leadFont)
                    .lineSpacing(titleIsDerived ? 4 : 7)
                    .foregroundStyle(Theme.ink)
                    .textSelection(.enabled)

                HStack(spacing: 10) {
                    Rectangle().fill(Theme.orange).frame(width: 9, height: 9).accessibilityHidden(true)
                    Text("\(count) \(count == 1 ? "WORD" : "WORDS") · \(max(1, count / 200)) MIN READ")
                        .font(.brandMono).tracking(0.8).foregroundStyle(Theme.muted)
                }

                TagFlow {
                    ForEach(item.tags, id: \.self) { tag in
                        Button { model.tagFilter = tag; model.filter = .everything; model.selectedTab = .memory; model.memoryPath = [] } label: { TagChip(tag: tag) }
                            .accessibilityHint("Shows notes with this tag")
                    }
                    Button { model.present(.tags(item.id)) } label: {
                        Label(item.tags.isEmpty ? "add tags" : "edit", systemImage: "number")
                            .font(.uiSubheadline.weight(.semibold)).foregroundStyle(Theme.ink)
                            .padding(.horizontal, 12).padding(.vertical, 7)
                            .background(Theme.surface, in: Capsule()).overlay(Capsule().stroke(Theme.border))
                    }
                }

                related(item)
            }
            .padding(.horizontal, 22)
            .padding(.top, 8)
            .padding(.bottom, 24)
        }
        .safeAreaInset(edge: .bottom) { actionBar(item) }
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { model.togglePin(item.id) } label: {
                    Label(item.pinned ? "Unpin" : "Pin", systemImage: item.pinned ? "pin.slash" : "pin")
                }
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

    @ViewBuilder private func related(_ item: MemoryItem) -> some View {
        let others = model.related(to: item)
        if !others.isEmpty {
            VStack(alignment: .leading, spacing: 12) {
                SectionTitle(text: "from your memory")
                ForEach(others) { other in
                    NavigationLink(value: other.route) { ItemRow(item: other) }.buttonStyle(.plain)
                }
            }
            .padding(.top, 8)
        }
    }

    /// Rewriting is the main thing to do with a note, so it is the one orange button.
    private func actionBar(_ item: MemoryItem) -> some View {
        HStack(spacing: 10) {
            if item.kind == .note {
                Button { model.present(.note(NoteDraft(text: item.body, recipientName: nil, existingID: item.id, tags: item.tags))) } label: {
                    Label("edit and rewrite", systemImage: "wand.and.stars")
                }
                .buttonStyle(.brandHot)
            }
            Button { model.present(.tags(item.id)) } label: { Image(systemName: "number").frame(width: 48, height: 48) }
                .accessibilityLabel("Tags")
            ShareLink(item: item.body) {
                Image(systemName: "square.and.arrow.up").frame(width: 48, height: 48)
            }
            .accessibilityLabel("Share")
            Menu {
                if item.kind == .note {
                    Menu("send with…") {
                        ForEach(ServiceKind.allCases) { service in
                            Button { send(item, via: service) } label: { Label(service.name, systemImage: service.symbol) }
                        }
                    }
                }
                Button("forget this one", role: .destructive) { confirmForget = true }
            } label: {
                Image(systemName: "ellipsis").frame(width: 48, height: 48)
            }
            .accessibilityLabel("More")
        }
        .font(.system(size: 17, weight: .semibold))
        .foregroundStyle(Theme.ink)
        .padding(8)
        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 26, style: .continuous))
        .shadow(color: Theme.ink.opacity(0.18), radius: 18, x: 0, y: 8)
        .padding(.horizontal, 22)
        .padding(.bottom, 6)
    }

    private func send(_ item: MemoryItem, via service: ServiceKind) {
        let candidates = Contacts.candidates(for: nil, service: service)
        model.startAction(ActionRequest(
            service: service, content: item.body,
            recipient: candidates.count == 1 ? candidates[0] : nil, candidates: candidates
        ))
    }
}
