import SwiftUI

struct MemoryView: View {
    @Environment(AppModel.self) private var model
    @State private var searching = false

    var body: some View {
        @Bindable var model = model
        NavigationStack(path: $model.memoryPath) {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text(model.warm("you said it. we kept it.", bro: "you said it, bro. we kept it."))
                        .font(.brandTitle).foregroundStyle(Theme.ink)
                        .accessibilityAddTraits(.isHeader)
                    SearchField(text: $model.query)
                    filters
                    tagChips
                    content
                }
                .padding(.horizontal, 20)
                .padding(.vertical, 12)
            }
            .screenBackground()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { Wordmark() }
                SettingsToolbar()
            }
            .navigationDestination(for: Route.self) { RouteDestination(route: $0) }
            // A short settle so a typed query shows "searching" instead of flicker.
            .task(id: model.query) {
                guard !model.query.isEmpty else { searching = false; return }
                searching = true
                try? await Task.sleep(for: .milliseconds(250))
                if !Task.isCancelled { searching = false }
            }
        }
    }

    private var filters: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(MemoryFilter.allCases) { f in
                    Button { model.filter = f } label: {
                        Text(f.rawValue)
                            .font(.uiSubheadline.weight(.semibold))
                            .padding(.horizontal, 14).padding(.vertical, 8)
                            .foregroundStyle(model.filter == f ? Color.white : Theme.ink)
                            .background(model.filter == f ? Theme.ink : Theme.surface, in: Capsule())
                            .overlay(Capsule().stroke(Theme.border))
                    }
                    .accessibilityAddTraits(model.filter == f ? .isSelected : [])
                }
            }
        }
    }

    @ViewBuilder private var tagChips: some View {
        let tags = model.allTags
        if !tags.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(tags, id: \.name) { tag in
                        let on = model.tagFilter == tag.name
                        Button { model.tagFilter = on ? nil : tag.name } label: {
                            Text("#\(tag.name) \(tag.count)")
                                .font(.uiSubheadline.weight(.semibold))
                                .padding(.horizontal, 12).padding(.vertical, 7)
                                .foregroundStyle(Theme.ink)
                                .background(on ? Theme.yellow : Theme.skySoft, in: Capsule())
                        }
                        .accessibilityAddTraits(on ? .isSelected : [])
                    }
                }
            }
        }
    }

    @ViewBuilder private var content: some View {
        let results = model.results
        if model.demo.searchUnavailable && !model.query.isEmpty {
            ContentUnavailableView {
                Label("couldn't search right now.", systemImage: "wifi.exclamationmark")
            } description: {
                Text("Your memories are safe. Try again in a moment.")
            } actions: {
                Button("clear search") { model.query = "" }.buttonStyle(.brandQuiet)
            }
        } else if searching {
            HStack(spacing: 8) { ProgressView(); Text("searching…").foregroundStyle(Theme.muted) }
                .frame(maxWidth: .infinity).padding(.top, 40)
        } else if model.items.isEmpty {
            ContentUnavailableView {
                Label(model.warm("nothing here yet.", bro: "nothing here yet, bro."), systemImage: "tray")
            } description: {
                Text("Say something and a note will land here.")
            } actions: {
                Button("go say something") { model.selectedTab = .speak }.buttonStyle(.brandPrimary)
            }
        } else if results.isEmpty {
            ContentUnavailableView {
                Label("not ringing a bell.", systemImage: "magnifyingglass")
            } description: {
                Text("No matches. Try a different word, or another filter.")
            } actions: {
                Button("clear search") { model.query = ""; model.filter = .everything; model.tagFilter = nil }.buttonStyle(.brandQuiet)
            }
        } else {
            LazyVStack(spacing: 12) {
                ForEach(results) { item in
                    NavigationLink(value: item.route) { ItemRow(item: item) }.buttonStyle(.plain)
                }
            }
        }
    }
}
