import SwiftUI

/// A removable tag chip. The × is a real button with its own label.
struct TagChip: View {
    let tag: String
    var onRemove: (() -> Void)?

    var body: some View {
        HStack(spacing: 6) {
            Text("#\(tag)").font(.uiSubheadline.weight(.semibold)).foregroundStyle(Theme.ink)
            if let onRemove {
                Button(action: onRemove) {
                    Image(systemName: "xmark").font(.system(size: 10, weight: .bold)).foregroundStyle(Theme.ink)
                        .frame(width: 22, height: 22)
                }
                .accessibilityLabel("Remove tag \(tag)")
            }
        }
        .padding(.leading, 12).padding(.trailing, onRemove == nil ? 12 : 4).padding(.vertical, 5)
        .background(Theme.skySoft, in: Capsule())
    }
}

/// Tags laid out in rows that wrap, since a note can carry any number of them.
struct TagFlow: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        arrange(width: proposal.width ?? .infinity, subviews: subviews).size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let result = arrange(width: bounds.width, subviews: subviews)
        for (index, origin) in result.origins.enumerated() {
            subviews[index].place(at: CGPoint(x: bounds.minX + origin.x, y: bounds.minY + origin.y), proposal: .unspecified)
        }
    }

    private func arrange(width: CGFloat, subviews: Subviews) -> (size: CGSize, origins: [CGPoint]) {
        var origins: [CGPoint] = []
        var x: CGFloat = 0, y: CGFloat = 0, rowHeight: CGFloat = 0, maxX: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x + size.width > width, x > 0 { x = 0; y += rowHeight + spacing; rowHeight = 0 }
            origins.append(CGPoint(x: x, y: y))
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
            maxX = max(maxX, x - spacing)
        }
        return (CGSize(width: maxX, height: y + rowHeight), origins)
    }
}

/// Adds and removes tags on one note. Existing tags are offered first so spelling stays consistent.
struct TagSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let itemID: UUID
    @State private var draft = ""
    @FocusState private var focused: Bool

    private var item: MemoryItem? { model.item(itemID) }
    private var applied: [String] { item?.tags ?? [] }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Text("tags.").font(.brandTitle).foregroundStyle(Theme.ink).accessibilityAddTraits(.isHeader)

                    HStack(spacing: 10) {
                        Text("#").font(.brandCard).foregroundStyle(Theme.muted)
                        TextField("add a tag", text: $draft)
                            .font(.uiBody)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .submitLabel(.done)
                            .focused($focused)
                            .onSubmit(add)
                        Button("add", action: add)
                            .font(.uiSubheadline.weight(.bold)).foregroundStyle(Theme.primary)
                            .disabled(AppModel.cleanTag(draft).isEmpty)
                    }
                    .padding(.horizontal, 14).padding(.vertical, 12)
                    .background(Theme.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(Theme.border))

                    if !applied.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("ON THIS NOTE").font(.brandMono).tracking(1).foregroundStyle(Theme.muted)
                            TagFlow { ForEach(applied, id: \.self) { tag in TagChip(tag: tag) { remove(tag) } } }
                        }
                    }

                    let suggestions = model.suggestTags(for: (item?.body ?? "") + " " + draft, excluding: applied)
                    if !suggestions.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("SUGGESTED").font(.brandMono).tracking(1).foregroundStyle(Theme.muted)
                            TagFlow {
                                ForEach(suggestions, id: \.self) { tag in
                                    Button { apply(tag) } label: {
                                        Text("+ #\(tag)").font(.uiSubheadline.weight(.semibold)).foregroundStyle(Theme.ink)
                                            .padding(.horizontal, 12).padding(.vertical, 8)
                                            .background(Theme.surface, in: Capsule()).overlay(Capsule().stroke(Theme.border))
                                    }
                                }
                            }
                        }
                    }
                }
                .padding(22)
            }
            .screenBackground()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }
    }

    private func add() { apply(draft); draft = "" }
    private func apply(_ tag: String) { model.setTags(itemID, applied + [tag]) }
    private func remove(_ tag: String) { model.setTags(itemID, applied.filter { $0 != tag }) }
}

/// Every tag with how often it is used. Rename merges two tags; delete only removes the label.
struct TagsView: View {
    @Environment(AppModel.self) private var model
    @State private var renaming: String?
    @State private var newName = ""
    @State private var deleting: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("tags.").font(.brandTitle).foregroundStyle(Theme.ink).accessibilityAddTraits(.isHeader)
                let tags = model.allTags
                if tags.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("no tags yet.").font(.brandHeading).foregroundStyle(Theme.ink)
                        Text("Add tags from a note's # button, or while you edit it.").foregroundStyle(Theme.muted)
                    }
                    .card()
                } else {
                    VStack(spacing: 0) {
                        ForEach(Array(tags.enumerated()), id: \.element.name) { index, tag in
                            HStack(spacing: 12) {
                                Text("#\(tag.name)").font(.uiHeadline).foregroundStyle(Theme.ink)
                                Spacer()
                                Text("\(tag.count) \(tag.count == 1 ? "note" : "notes")").font(.brandMono).foregroundStyle(Theme.muted)
                                Menu {
                                    Button("Show notes") { model.tagFilter = tag.name; model.filter = .everything; model.selectedTab = .memory; model.sheet = nil }
                                    Button("Rename") { newName = tag.name; renaming = tag.name }
                                    Button("Delete tag", role: .destructive) { deleting = tag.name }
                                } label: {
                                    Image(systemName: "ellipsis").frame(width: 36, height: 36).foregroundStyle(Theme.muted)
                                }
                                .accessibilityLabel("Actions for \(tag.name)")
                            }
                            .padding(.horizontal, 16).padding(.vertical, 8)
                            if index < tags.count - 1 { Divider().padding(.leading, 16) }
                        }
                    }
                    .background(Theme.surface, in: RoundedRectangle(cornerRadius: Theme.cardRadius, style: .continuous))
                    .shadow(color: Theme.ink.opacity(0.10), radius: 14, x: 0, y: 8)
                    Text("Renaming a tag to one that exists merges them.").font(.uiFootnote).foregroundStyle(Theme.muted)
                }
            }
            .padding(22)
        }
        .screenBackground()
        .navigationBarTitleDisplayMode(.inline)
        .alert("Rename tag", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
            TextField("tag", text: $newName).textInputAutocapitalization(.never)
            Button("Rename") { if let old = renaming { model.renameTag(old, to: newName) }; renaming = nil }
            Button("Cancel", role: .cancel) { renaming = nil }
        }
        .confirmationDialog("Delete this tag?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }), titleVisibility: .visible) {
            Button("Delete tag", role: .destructive) { if let name = deleting { model.deleteTag(name) }; deleting = nil }
            Button("Cancel", role: .cancel) { deleting = nil }
        } message: {
            Text("Your notes stay. Only the label is removed from them.")
        }
    }
}
