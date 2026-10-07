import SwiftUI

/// Night sky and cool paper, taken from the logo itself. The guide's warm cream canvas was
/// replaced with a blue-leaning paper so the surfaces belong to the artwork. Orange and
/// yellow are fills only, always under an ink label, never text on white.
enum Theme {
    static let canvas = Color(hex: 0xEDF1F9)   // paper
    static let surface = Color.white
    static let ink = Color(hex: 0x0B1547)
    static let night = Color(hex: 0x070E33)
    static let primary = Color(hex: 0x0243B2)  // links and small accents on paper
    static let sky = Color(hex: 0x0D84FE)
    static let skySoft = Color(hex: 0xD3E6FF)
    static let orange = Color(hex: 0xFF981A)
    static let yellow = Color(hex: 0xFFE76A)
    static let muted = Color(hex: 0x59647F)
    static let border = Color(hex: 0xDAE1EF)
    static let error = Color(hex: 0xB42318)

    static let cardRadius: CGFloat = 22
}

extension Color {
    init(hex: UInt32) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: 1
        )
    }
}

/// Bricolage Grotesque speaks (headings, key labels), Geist reads, Geist Mono labels.
/// All three are bundled and registered at launch (Resources/Fonts, see Fonts.register).
extension Font {
    private static let display = "Bricolage Grotesque"
    private static let text = "Geist"
    private static let mono = "Geist Mono"

    static let brandTitle = Font.custom(display, size: 40, relativeTo: .largeTitle).weight(.heavy)
    static let brandHeading = Font.custom(display, size: 22, relativeTo: .title3).weight(.bold)
    static let brandCard = Font.custom(display, size: 18, relativeTo: .headline).weight(.bold)
    /// Small status labels only, never paragraphs or names.
    static let brandMono = Font.custom(mono, size: 12, relativeTo: .footnote).weight(.medium)

    static let uiBody = Font.custom(text, size: 16, relativeTo: .body)
    static let uiHeadline = Font.custom(text, size: 17, relativeTo: .headline).weight(.semibold)
    static let uiSubheadline = Font.custom(text, size: 15, relativeTo: .subheadline)
    static let uiFootnote = Font.custom(text, size: 13, relativeTo: .footnote)
    static let uiCaption = Font.custom(text, size: 12, relativeTo: .caption)
}

extension View {
    func card(padding: CGFloat = 16) -> some View {
        self
            .padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.surface, in: RoundedRectangle(cornerRadius: Theme.cardRadius, style: .continuous))
            .shadow(color: Theme.ink.opacity(0.10), radius: 14, x: 0, y: 8)
    }

    func screenBackground() -> some View {
        background(Theme.canvas.ignoresSafeArea())
    }
}

/// The agreed relative-time wording: "just now" under a minute, then the system's.
func ago(_ date: Date) -> String {
    if Date.now.timeIntervalSince(date) < 60 { return "just now" }
    return RelativeDateTimeFormatter().localizedString(for: date, relativeTo: .now)
}

func clock(_ seconds: Int) -> String {
    String(format: "%d:%02d", seconds / 60, seconds % 60)
}
