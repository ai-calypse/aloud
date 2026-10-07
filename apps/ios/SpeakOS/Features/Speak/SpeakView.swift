import SwiftUI

struct SpeakView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        NavigationStack(path: $model.speakPath) {
            ScrollView {
                VStack(spacing: 28) {
                    VStack(spacing: 4) {
                        Text("say anything.").font(.brandTitle).foregroundStyle(Theme.ink)
                        Text(model.warm("seriously.", bro: "seriously, bro.")).font(.brandHeading).foregroundStyle(Theme.muted)
                    }
                    .multilineTextAlignment(.center)
                    .accessibilityElement(children: .combine)
                    .accessibilityAddTraits(.isHeader)

                    SpeakControl()
                    CaptureStatusView()

                    if let saved = model.lastSaved { RememberedCard(item: saved) }

                    SecondaryActions()
                    RecentSection()
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
        }
    }
}

private struct SecondaryActions: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        // Stacks under larger text sizes instead of shrinking the labels.
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 12) { buttons }
            VStack(spacing: 12) { buttons }
        }
        .buttonStyle(.bordered)
        .controlSize(.large)
        .disabled(model.phase != .idle)
    }

    @ViewBuilder private var buttons: some View {
        Button { model.startMeeting() } label: { Label("start meeting", systemImage: "person.2").frame(maxWidth: .infinity) }
        Button { model.present(.typed(prefill: "")) } label: { Label("type instead", systemImage: "keyboard").frame(maxWidth: .infinity) }
    }
}

private struct RecentSection: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(model.warm("your brain, lately.", bro: "your brain, lately, bro."))
                .font(.brandHeading).foregroundStyle(Theme.ink)
                .accessibilityAddTraits(.isHeader)

            if model.items.isEmpty {
                // First use: never fake memories in a real, empty account.
                HStack(spacing: 14) {
                    PixelSpark()
                    VStack(alignment: .leading, spacing: 2) {
                        Text(model.warm("a little quiet in here.", bro: "a little quiet in here, bro.")).font(.headline).foregroundStyle(Theme.ink)
                        Text("say a thought. we'll keep it.").font(.subheadline).foregroundStyle(Theme.muted)
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

            Button { model.selectedTab = .memory } label: {
                Text("search everything you've said →").font(.subheadline.weight(.semibold))
            }
            .padding(.top, 4)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
