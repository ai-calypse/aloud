import SwiftUI

/// What sits under the hero: examples while idle, the live transcript, processing, or the retry card.
/// The state itself (listening, got it) is named by the pill on the hero.
struct CaptureStatusView: View {
    @Environment(AppModel.self) private var model

    private static let examples = [
        "“what did I say about pricing?”",
        "“remember that Sarah likes the green version”",
        "“write Sarah a message saying I'll be there at eight”",
        "“send Sarah a Slack saying I'm running late”",
    ]

    private let transcriptFont = Font.custom("Bricolage Grotesque", size: 25, relativeTo: .title2).weight(.bold)

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            switch model.phase {
            case .idle: idle
            case .listening(let armed): listening(armed: armed)
            case .processing: processing
            case .failed(let text): failed(text)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var idle: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("TRY SAYING").font(.brandMono).tracking(1).foregroundStyle(Theme.muted)
            // Rotates only while idle, so it never competes with a transcript or a result.
            TimelineView(.periodic(from: .now, by: 6)) { ctx in
                let i = Int(ctx.date.timeIntervalSinceReferenceDate / 6) % Self.examples.count
                Text(Self.examples[i])
                    .font(.custom("Geist Mono", size: 13, relativeTo: .footnote))
                    .foregroundStyle(Theme.ink)
                    .padding(.horizontal, 11).padding(.vertical, 8)
                    .background(Theme.skySoft, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .id(i)
                    .transition(.opacity)
            }
            if model.tapToRecord {
                Button { model.beginCapture() } label: { Label("start", systemImage: "mic.fill") }
                    .buttonStyle(.brandPrimary).padding(.top, 6)
            }
        }
        .frame(minHeight: 84, alignment: .top)
    }

    private func listening(armed: Bool) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            // Reserved height so words arriving never move what is below.
            Text(model.liveTranscript.isEmpty ? " " : model.liveTranscript)
                .font(transcriptFont).foregroundStyle(Theme.ink)
                .lineLimit(3, reservesSpace: true)
                .accessibilityHidden(true) // partial words are not announced one by one
            if model.tapToRecord {
                HStack(spacing: 12) {
                    Button { model.releaseCapture() } label: { Label("stop", systemImage: "stop.fill") }
                        .buttonStyle(.brandPrimary)
                    Button("cancel", role: .cancel) { model.cancelCapture() }
                        .buttonStyle(.brandQuiet)
                }
            } else {
                Text((armed ? "slide back to keep going" : "release when you're done · slide left to cancel").uppercased())
                    .font(.brandMono).tracking(0.8).foregroundStyle(Theme.muted)
            }
        }
        .frame(minHeight: 84, alignment: .top)
    }

    private var processing: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 10) {
                ProgressView()
                Text(model.processingSlow ? "still working on that…" : "one sec…").font(transcriptFont).foregroundStyle(Theme.ink)
            }
            Button("cancel") { model.cancelProcessing() }.buttonStyle(.brandQuiet).fixedSize()
        }
        .frame(minHeight: 84, alignment: .top)
        .accessibilityElement(children: .combine)
    }

    private func failed(_ text: String) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("couldn't finish that. your text is still here.")
                .font(.brandCard).foregroundStyle(Theme.ink)
            Text(text).font(.uiBody).foregroundStyle(Theme.ink)
                .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                .background(Theme.canvas, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            InlineActions {
                Button("try again") { model.submit(text) }.buttonStyle(.brandHot)
                Button("edit text") { model.discardFailed(); model.present(.typed(prefill: text)) }
                Button("discard", role: .destructive) { model.discardFailed() }
            }
        }
        .multilineTextAlignment(.leading)
        .card()
    }
}

/// "remembered." stays until dismissed or the next capture starts; edit and undo never race a timeout.
struct RememberedCard: View {
    @Environment(AppModel.self) private var model
    let item: MemoryItem

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Rectangle().fill(Theme.orange).frame(width: 10, height: 10).accessibilityHidden(true)
                Text("remembered.").font(.brandHeading).foregroundStyle(.white)
            }
            Text(item.body).font(.uiBody).foregroundStyle(.white)
            Text("THOUGHT · \(ago(item.date).uppercased())").font(.brandMono).tracking(0.8).foregroundStyle(Color(hex: 0xAAB6DC))
            InlineActions {
                Button("edit") { model.present(.thoughtEditor(item.id)) }
                Button("undo") { model.undoLastSaved() }
                Button("done") { model.lastSaved = nil }.buttonStyle(.brandHot)
            }
            .padding(.top, 4)
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Theme.ink, in: RoundedRectangle(cornerRadius: Theme.cardRadius, style: .continuous))
        .accessibilityElement(children: .contain)
    }
}
