import SwiftUI

/// The one big control: the final logo, intact, as the hold-to-talk target.
/// The character art is never redrawn or animated; only the ring and halo around
/// it change, because moving eyes or headphones would need new source frames.
struct SpeakControl: View {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @ScaledMetric(relativeTo: .largeTitle) private var size: CGFloat = 190
    @State private var pressing = false

    private let cancelDistance: CGFloat = 80

    private var listening: Bool {
        if case .listening = model.phase { return true }
        return false
    }
    private var armed: Bool {
        if case .listening(let a) = model.phase { return a }
        return false
    }

    var body: some View {
        ZStack {
            halo
            tile
        }
        .frame(width: size + 40, height: size + 40)
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { value in
                    guard !model.tapToRecord else { return }
                    if !pressing {
                        pressing = true
                        model.beginCapture()
                    }
                    model.setArmed(value.translation.width < -cancelDistance)
                }
                .onEnded { value in
                    if model.tapToRecord {
                        if abs(value.translation.width) < 10, abs(value.translation.height) < 10 { model.toggleTapRecording() }
                        return
                    }
                    pressing = false
                    model.releaseCapture()
                }
        )
        .disabled(isBusy)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(listening ? "Recording" : "Hold to talk")
        .accessibilityHint(model.tapToRecord ? "Double tap to start or stop recording" : "Press and hold to speak, then release")
        .accessibilityAddTraits(.isButton)
        // VoiceOver cannot hold, so its default action always toggles recording.
        .accessibilityAction { model.toggleTapRecording() }
        .accessibilityAction(named: "Cancel recording") { model.cancelCapture() }
    }

    private var isBusy: Bool {
        switch model.phase {
        case .processing, .failed: true
        default: false
        }
    }

    private var tile: some View {
        Image("SpeakosLogo")
            .resizable()
            .interpolation(.none) // keep the pixel edges sharp
            .scaledToFill()
            .frame(width: size, height: size)
            .clipShape(RoundedRectangle(cornerRadius: 30, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 30, style: .continuous)
                    .strokeBorder(ringColor, lineWidth: listening ? 6 : 1)
            )
            .overlay(alignment: .topLeading) {
                if armed {
                    Image(systemName: "xmark.circle.fill")
                        .font(.title)
                        .symbolRenderingMode(.palette)
                        .foregroundStyle(.white, Theme.error)
                        .padding(8)
                        .accessibilityHidden(true)
                }
            }
            .scaleEffect(pressing && !reduceMotion ? 0.97 : 1)
            .animation(.spring(duration: 0.25, bounce: 0.3), value: pressing)
    }

    private var ringColor: Color {
        if armed { return Theme.error }
        return listening ? Theme.orange : Theme.border
    }

    @ViewBuilder private var halo: some View {
        if listening {
            TimelineView(.animation(paused: reduceMotion)) { ctx in
                let t = ctx.date.timeIntervalSinceReferenceDate
                RoundedRectangle(cornerRadius: 40, style: .continuous)
                    .fill((armed ? Theme.error : Theme.orange).opacity(0.28))
                    .frame(width: size + 28, height: size + 28)
                    .scaleEffect(reduceMotion ? 1 : 1 + 0.04 * sin(t * 5))
                    .blur(radius: 10)
            }
            .accessibilityHidden(true)
        }
    }
}
