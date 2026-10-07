import SwiftUI

/// What sits under the control: idle prompt, listening, processing or the retry card.
struct CaptureStatusView: View {
    @Environment(AppModel.self) private var model

    private static let examples = [
        "try: “what did I say about pricing?”",
        "try: “remember that Sarah likes the green version”",
        "try: “write Sarah a message saying I'll be there at eight”",
        "try: “send Sarah a Slack saying I'm running late”",
    ]

    var body: some View {
        VStack(spacing: 10) {
            switch model.phase {
            case .idle: idle
            case .listening(let armed): listening(armed: armed)
            case .processing: processing
            case .failed(let text): failed(text)
            }
        }
        .frame(maxWidth: .infinity)
        .multilineTextAlignment(.center)
    }

    private var idle: some View {
        VStack(spacing: 6) {
            Text(model.tapToRecord ? "tap to talk" : "hold to talk")
                .font(.brandHeading).foregroundStyle(Theme.ink)
            Text("we'll figure out what you mean.").font(.subheadline).foregroundStyle(Theme.muted)
            // Rotates only while idle, so it never competes with a transcript or a result.
            TimelineView(.periodic(from: .now, by: 6)) { ctx in
                let i = Int(ctx.date.timeIntervalSinceReferenceDate / 6) % Self.examples.count
                Text(Self.examples[i]).font(.brandMono).foregroundStyle(Theme.muted)
                    .id(i)
                    .transition(.opacity)
            }
            .padding(.top, 2)
            if model.tapToRecord {
                Button { model.beginCapture() } label: { Label("start", systemImage: "mic.fill") }
                    .buttonStyle(.borderedProminent).controlSize(.large)
                    .padding(.top, 6)
            }
        }
    }

    private func listening(armed: Bool) -> some View {
        VStack(spacing: 10) {
            if armed {
                Label("release to cancel", systemImage: "xmark.circle.fill")
                    .font(.brandMono).foregroundStyle(Theme.error)
            } else {
                Label("listening…", systemImage: "record.circle")
                    .font(.brandMono).foregroundStyle(Theme.ink)
            }
            Waveform(active: !armed)
            // Reserved height so words arriving never move what is below.
            Text(model.liveTranscript.isEmpty ? " " : model.liveTranscript)
                .font(.body).foregroundStyle(Theme.ink)
                .lineLimit(3, reservesSpace: true)
                .accessibilityHidden(true) // partial words are not announced one by one
            if model.tapToRecord {
                HStack(spacing: 12) {
                    Button { model.releaseCapture() } label: { Label("stop", systemImage: "stop.fill") }
                        .buttonStyle(.borderedProminent)
                    Button("cancel", role: .cancel) { model.cancelCapture() }
                        .buttonStyle(.bordered)
                }
                .controlSize(.large)
            } else {
                Text(armed ? "slide back to keep going" : "release when you're done")
                    .font(.subheadline.weight(.semibold)).foregroundStyle(Theme.ink)
                if !armed { Text("slide left to cancel").font(.footnote).foregroundStyle(Theme.muted) }
            }
        }
    }

    private var processing: some View {
        VStack(spacing: 8) {
            Text("got it.").font(.brandHeading).foregroundStyle(Theme.ink)
            HStack(spacing: 8) {
                ProgressView()
                Text(model.processingSlow ? "still working on that…" : "one sec…").foregroundStyle(Theme.muted)
            }
            Button("cancel") { model.cancelProcessing() }.buttonStyle(.bordered).padding(.top, 4)
        }
        .accessibilityElement(children: .combine)
    }

    private func failed(_ text: String) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("couldn't finish that. your text is still here.")
                .font(.headline).foregroundStyle(Theme.ink)
            Text(text).font(.body).foregroundStyle(Theme.ink)
                .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                .background(Theme.canvas, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            InlineActions {
                Button("try again") { model.submit(text) }
                    .buttonStyle(.borderedProminent)
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
                PixelSpark(color: Theme.orange)
                Text("remembered.").font(.brandHeading).foregroundStyle(Theme.ink)
            }
            Text(item.body).font(.body).foregroundStyle(Theme.ink)
            Text("thought · \(ago(item.date))").font(.brandMono).foregroundStyle(Theme.muted)
            InlineActions {
                Button("edit") { model.present(.thoughtEditor(item.id)) }
                Button("undo") { model.undoLastSaved() }
                Button("done") { model.lastSaved = nil }.buttonStyle(.borderedProminent)
            }
        }
        .card()
        .accessibilityElement(children: .contain)
    }
}
