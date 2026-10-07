import SwiftUI

/// The sky window: the final logo, intact, edge to edge, and the hold-to-talk target.
/// The character art is never redrawn or animated. While listening it dims under a night
/// veil and a pixel meter rises in front of it, because moving the eyes or headphones would
/// need new source frames. The pill straddling its lower edge says what state the app is in.
struct SpeakHero: View {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var pressing = false

    private let cancelDistance: CGFloat = 80
    private let height: CGFloat = 440

    private var listening: Bool {
        if case .listening = model.phase { return true }
        return false
    }
    private var armed: Bool {
        if case .listening(let a) = model.phase { return a }
        return false
    }
    private var busy: Bool {
        switch model.phase {
        case .processing, .failed: true
        default: false
        }
    }

    var body: some View {
        ZStack(alignment: .bottom) {
            art
            LinearGradient(
                colors: [(armed ? Color(hex: 0x5A0C08) : Theme.night).opacity(0.15), (armed ? Color(hex: 0x5A0C08) : Theme.night).opacity(0.9)],
                startPoint: .top, endPoint: .bottom
            )
            .opacity(listening ? 1 : 0)
            .animation(.easeOut(duration: 0.2), value: listening)

            if listening {
                Waveform(active: true, color: armed ? Color(hex: 0xFF6B5E) : Theme.orange, height: 96)
                    .padding(.horizontal, 24)
                    .padding(.bottom, 54)
            }
        }
        .frame(height: height)
        .frame(maxWidth: .infinity)
        .clipShape(UnevenRoundedRectangle(bottomLeadingRadius: 40, bottomTrailingRadius: 40, style: .continuous))
        .overlay(alignment: .bottom) { pill.offset(y: 27) }
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { value in
                    guard !model.tapToRecord, !busy else { return }
                    if !pressing {
                        pressing = true
                        model.beginCapture()
                    }
                    model.setArmed(value.translation.width < -cancelDistance)
                }
                .onEnded { value in
                    guard !busy else { return }
                    if model.tapToRecord {
                        if abs(value.translation.width) < 10, abs(value.translation.height) < 10 { model.toggleTapRecording() }
                        return
                    }
                    pressing = false
                    model.releaseCapture()
                }
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(listening ? "Recording" : (model.tapToRecord ? "Tap to talk" : "Hold to talk"))
        .accessibilityHint(model.tapToRecord ? "Double tap to start or stop recording" : "Press and hold to speak, then release")
        .accessibilityAddTraits(.isButton)
        // VoiceOver cannot hold, so its default action always toggles recording.
        .accessibilityAction { model.toggleTapRecording() }
        .accessibilityAction(named: "Cancel recording") { model.cancelCapture() }
    }

    private var art: some View {
        Theme.sky
            .overlay(
                Image("SpeakosLogo")
                    .resizable()
                    .interpolation(.none) // keep the pixel edges sharp
                    .scaledToFill()
                    .offset(y: 24)
                    .scaleEffect(pressing && !reduceMotion ? 1.03 : 1)
                    .animation(.easeOut(duration: 0.4), value: pressing)
            )
            .clipped()
    }

    @ViewBuilder private var pill: some View {
        let label = pillLabel
        HStack(spacing: 10) {
            if label.live {
                Rectangle().fill(Theme.orange).frame(width: 9, height: 9)
            } else {
                Image(systemName: "mic.fill")
            }
            Text(label.text).font(.custom("Bricolage Grotesque", size: 17, relativeTo: .headline).weight(.bold))
        }
        .foregroundStyle(label.fg)
        .padding(.horizontal, 24)
        .frame(height: 54)
        .background(label.bg, in: Capsule())
        .overlay(Capsule().stroke(Theme.canvas, lineWidth: 6))
        .shadow(color: Theme.orange.opacity(label.glow ? 0.55 : 0), radius: 14, x: 0, y: 8)
        .accessibilityHidden(true)
    }

    private var pillLabel: (text: String, live: Bool, fg: Color, bg: Color, glow: Bool) {
        switch model.phase {
        case .listening(let armed):
            if armed { return ("release to cancel", false, Theme.error, Color.white, false) }
            return ("listening", true, Color.white, Theme.ink, false)
        case .processing: return ("got it", true, Color.white, Theme.ink, false)
        case .failed: return ("couldn't finish", false, Theme.error, Color.white, false)
        case .idle: return (model.tapToRecord ? "tap to talk" : "hold to talk", false, Theme.ink, Theme.orange, true)
        }
    }
}
