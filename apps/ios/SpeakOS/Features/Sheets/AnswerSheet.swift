import SwiftUI

/// A memory answer with the sources it was built from. The sheet owns its own navigation
/// stack, so opening a source and coming back lands on the same answer.
struct AnswerSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var answer: AnswerModel

    init(answer: AnswerModel) { _answer = State(initialValue: answer) }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text("“\(answer.question)”").font(.uiSubheadline).foregroundStyle(Theme.muted)
                    switch answer.state {
                    case .answer(let text): found(text)
                    case .noMatch: noMatch
                    case .unavailable: unavailable
                    }
                }
                .padding(20)
            }
            .screenBackground()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } } }
            .navigationDestination(for: Route.self) { RouteDestination(route: $0) }
        }
    }

    private func found(_ text: String) -> some View {
        let sources = answer.sources.compactMap { model.item($0) }
        return VStack(alignment: .leading, spacing: 16) {
            Text("here's what you said.").font(.brandHeading).foregroundStyle(Theme.ink)
                .accessibilityAddTraits(.isHeader)
            Text(text).font(.uiBody).foregroundStyle(Theme.ink).card()
            Text("\(sources.count) \(sources.count == 1 ? "memory" : "memories") used").font(.brandMono).foregroundStyle(Theme.muted)
            ForEach(sources) { item in
                NavigationLink(value: item.route) { ItemRow(item: item) }.buttonStyle(.plain)
            }
        }
    }

    private var noMatch: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("not ringing a bell.").font(.brandHeading).foregroundStyle(Theme.ink)
            Text("We found nothing saved that matches. try a different word or question.").foregroundStyle(Theme.muted)
            Button { model.present(.typed(prefill: answer.question)) } label: { Text("ask it differently").frame(maxWidth: .infinity) }
                .buttonStyle(.brandPrimary)
            Button {
                model.query = ""
                model.filter = .everything
                model.selectedTab = .memory
                dismiss()
            } label: { Text("search Memory").frame(maxWidth: .infinity) }
                .buttonStyle(.brandQuiet)
        }
        .controlSize(.large)
    }

    // Different from "no match": the search itself did not run, so we say nothing about what is saved.
    private var unavailable: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("couldn't search right now.").font(.brandHeading).foregroundStyle(Theme.ink)
            Text("Your memories are safe. This is a problem reaching search, not an empty result.").foregroundStyle(Theme.muted)
            Button {
                answer = Interpreter.answer(to: answer.question, items: model.items, searchUnavailable: model.demo.searchUnavailable)
            } label: { Text("try again").frame(maxWidth: .infinity) }
                .buttonStyle(.brandPrimary)
        }
        .controlSize(.large)
    }
}
