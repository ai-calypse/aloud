import SwiftUI

struct SpeakView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        NavigationStack(path: $model.speakPath) {
            ScrollView {
                VStack(spacing: 0) {
                    SpeakHero()
                    VStack(alignment: .leading, spacing: 26) {
                        VStack(alignment: .leading, spacing: 6) {
                            Text("say anything.").font(.brandTitle).foregroundStyle(Theme.ink)
                            Text(model.warm("seriously.", bro: "seriously, bro."))
                                .font(.custom("Bricolage Grotesque", size: 21, relativeTo: .title3).weight(.semibold))
                                .foregroundStyle(Theme.muted)
                        }
                        .accessibilityElement(children: .combine)
                        .accessibilityAddTraits(.isHeader)

                        CaptureStatusView()
                        if let saved = model.lastSaved { RememberedCard(item: saved) }
                        SecondaryActions()
                        RecentSection()
                    }
                    .padding(.horizontal, 22)
                    .padding(.top, 48)
                    .padding(.bottom, 32)
                }
            }
            .ignoresSafeArea(.container, edges: .top)
            .background(Theme.canvas.ignoresSafeArea())
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(.hidden, for: .navigationBar)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { Wordmark() }
                SettingsToolbar()
            }
            .navigationDestination(for: Route.self) { RouteDestination(route: $0) }
        }
    }
}

private struct SecondaryActions: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        // Stacks under larger text sizes instead of shrinking the labels.
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 12) { tiles }
            VStack(spacing: 12) { tiles }
        }
        .disabled(model.phase != .idle)
    }

    @ViewBuilder private var tiles: some View {
        tile("start meeting", symbol: "person.2.wave.2", dark: true) { model.startMeeting() }
        tile("type instead", symbol: "keyboard", dark: false) { model.present(.typed(prefill: "")) }
    }

    private func tile(_ title: String, symbol: String, dark: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 18) {
                Image(systemName: symbol).font(.system(size: 20, weight: .semibold))
                Text(title).font(.custom("Bricolage Grotesque", size: 16, relativeTo: .headline).weight(.bold))
            }
            .foregroundStyle(dark ? Color.white : Theme.ink)
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(dark ? Theme.ink : Theme.surface, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).stroke(dark ? Color.clear : Theme.border))
        }
        .buttonStyle(.plain)
    }
}

private struct RecentSection: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline) {
                SectionTitle(text: model.warm("your brain, lately", bro: "your brain, lately, bro"))
                Spacer()
                Button { model.selectedTab = .memory } label: {
                    Text("search all →").font(.uiSubheadline.weight(.semibold)).foregroundStyle(Theme.primary)
                }
            }

            if model.items.isEmpty {
                // First use: never fake memories in a real, empty account.
                HStack(spacing: 14) {
                    PixelSpark()
                    VStack(alignment: .leading, spacing: 2) {
                        Text(model.warm("a little quiet in here.", bro: "a little quiet in here, bro.")).font(.uiHeadline).foregroundStyle(Theme.ink)
                        Text("say a thought. we'll keep it.").font(.uiSubheadline).foregroundStyle(Theme.muted)
                    }
                }
                .card()
                .accessibilityElement(children: .combine)
            } else {
                ForEach(model.recent) { item in
                    NavigationLink(value: item.route) { ItemRow(item: item) }
                        .buttonStyle(.plain)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
