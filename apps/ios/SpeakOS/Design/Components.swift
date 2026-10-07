import SwiftUI
import UIKit
import CoreText

enum Haptics {
    static func start() { UIImpactFeedbackGenerator(style: .medium).impactOccurred() }
    static func tick() { UIImpactFeedbackGenerator(style: .light).impactOccurred() }
    static func success() { UINotificationFeedbackGenerator().notificationOccurred(.success) }
}

enum Fonts {
    /// Registers the bundled variable fonts so no Info.plist entry is needed.
    /// If a face looks like the system font on device, print `UIFont.familyNames` and match the names in Theme.
    static func register() {
        let urls = Bundle.main.urls(forResourcesWithExtension: "ttf", subdirectory: nil) ?? []
        for url in urls { CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil) }
    }
}

// MARK: Buttons

/// The one loud action on a screen: ink fill, white label.
struct BrandPrimaryStyle: ButtonStyle {
    @Environment(\.isEnabled) private var enabled
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.uiHeadline)
            .foregroundStyle(.white)
            .padding(.vertical, 14).padding(.horizontal, 18)
            .frame(maxWidth: .infinity)
            .background(Theme.ink, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .opacity(enabled ? (configuration.isPressed ? 0.85 : 1) : 0.4)
    }
}

/// White, hairline, ink label. Destructive role turns the label red.
struct BrandQuietStyle: ButtonStyle {
    @Environment(\.isEnabled) private var enabled
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.uiHeadline)
            .foregroundStyle(configuration.role == .destructive ? Theme.error : Theme.ink)
            .padding(.vertical, 12).padding(.horizontal, 16)
            .frame(maxWidth: .infinity)
            .background(Theme.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(Theme.border, lineWidth: 1))
            .opacity(enabled ? (configuration.isPressed ? 0.8 : 1) : 0.4)
    }
}

/// Orange, ink label. At most one per screen (the moment of commitment: send, start, go).
struct BrandHotStyle: ButtonStyle {
    @Environment(\.isEnabled) private var enabled
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.custom("Bricolage Grotesque", size: 17, relativeTo: .headline).weight(.bold))
            .foregroundStyle(Theme.ink)
            .padding(.vertical, 15).padding(.horizontal, 18)
            .frame(maxWidth: .infinity)
            .background(Theme.orange, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .opacity(enabled ? (configuration.isPressed ? 0.85 : 1) : 0.4)
    }
}

extension ButtonStyle where Self == BrandPrimaryStyle { static var brandPrimary: BrandPrimaryStyle { .init() } }
extension ButtonStyle where Self == BrandQuietStyle { static var brandQuiet: BrandQuietStyle { .init() } }
extension ButtonStyle where Self == BrandHotStyle { static var brandHot: BrandHotStyle { .init() } }

// MARK: Pieces

struct Wordmark: View {
    var body: some View {
        Text("speakos")
            .font(.custom("Bricolage Grotesque", size: 22, relativeTo: .title3).weight(.heavy))
            .foregroundStyle(Theme.ink)
    }
}

/// Kind label for a memory: a small chip glyph plus the word, so it never relies on colour.
struct KindTag: View {
    let kind: MemoryKind

    private var chip: (fill: Color, ink: Color) {
        switch kind {
        case .note: (Theme.skySoft, Theme.ink)
        case .meeting: (Theme.ink, .white)
        case .action: (Theme.yellow, Theme.ink)
        }
    }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: kind.symbol)
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(chip.ink)
                .frame(width: 20, height: 20)
                .background(chip.fill, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                .accessibilityHidden(true)
            Text(kind.label.uppercased()).font(.brandMono).tracking(0.8).foregroundStyle(Theme.ink)
        }
    }
}

struct ItemRow: View {
    let item: MemoryItem
    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack {
                KindTag(kind: item.kind)
                if item.pinned { Image(systemName: "pin.fill").font(.caption2).foregroundStyle(Theme.muted).accessibilityLabel("pinned") }
                Spacer()
                Text(ago(item.date)).font(.brandMono).foregroundStyle(Theme.muted)
            }
            Text(item.title).font(.brandCard).foregroundStyle(Theme.ink).multilineTextAlignment(.leading)
            if !item.body.isEmpty, item.body != item.title {
                Text(item.body).font(.uiSubheadline).foregroundStyle(Theme.muted).lineLimit(2).multilineTextAlignment(.leading)
            }
        }
        .card()
        .accessibilityElement(children: .combine)
    }
}

/// Section heading with the brand's one repeated mark: a small orange pixel.
struct SectionTitle: View {
    let text: String
    var body: some View {
        HStack(spacing: 9) {
            Rectangle().fill(Theme.orange).frame(width: 9, height: 9).accessibilityHidden(true)
            Text(text).font(.brandHeading).foregroundStyle(Theme.ink)
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}

/// A tiny plus-shaped pixel flourish. Decoration only: hidden from VoiceOver.
struct PixelSpark: View {
    var color: Color = Theme.orange
    var cell: CGFloat = 4

    var body: some View {
        VStack(spacing: 0) {
            row(0, 1, 0)
            row(1, 1, 1)
            row(0, 1, 0)
        }
        .accessibilityHidden(true)
    }

    private func row(_ a: Int, _ b: Int, _ c: Int) -> some View {
        HStack(spacing: 0) {
            ForEach([a, b, c].indices, id: \.self) { i in
                Rectangle().fill([a, b, c][i] == 1 ? color : Color.clear).frame(width: cell, height: cell)
            }
        }
    }
}

/// Live input level drawn as stacks of square pixels, like the logo's own art.
/// Static when reduced motion is on; the label carries the state.
struct Waveform: View {
    var active: Bool
    var color: Color = Theme.orange
    var height: CGFloat = 80
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private let columns = 22
    private let cell: CGFloat = 7
    private let gap: CGFloat = 2

    var body: some View {
        TimelineView(.animation(minimumInterval: 0.09, paused: !active || reduceMotion)) { context in
            let t = context.date.timeIntervalSinceReferenceDate
            HStack(alignment: .bottom, spacing: 5) {
                ForEach(0..<columns, id: \.self) { i in
                    let wave = abs(sin(t * 5 + Double(i) * 0.7)) * abs(cos(Double(i) * 0.55 + t))
                    let blocks = (active && !reduceMotion) ? 1 + Int((wave * 9).rounded()) : 3
                    VStack(spacing: gap) {
                        ForEach(0..<blocks, id: \.self) { _ in
                            Rectangle().fill(active ? color : Theme.border).frame(height: cell)
                        }
                    }
                    .frame(maxWidth: .infinity)
                }
            }
            .frame(height: height, alignment: .bottom)
        }
        .accessibilityHidden(true)
    }
}

struct ToastView: View {
    let text: String
    var body: some View {
        Text(text)
            .font(.uiSubheadline.weight(.bold))
            .foregroundStyle(Theme.ink)
            .padding(.horizontal, 18)
            .padding(.vertical, 10)
            .background(Theme.yellow, in: Capsule())
            .shadow(color: Theme.ink.opacity(0.25), radius: 10, x: 0, y: 6)
            .padding(.top, 8)
    }
}

/// Small action row used under cards (edit · undo · done).
struct InlineActions<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 8) { content }
            VStack(alignment: .leading, spacing: 8) { content }
        }
        .buttonStyle(.brandQuiet)
    }
}

/// The search field every root page carries. One query, shared by all pages, so a search follows you.
struct SearchField: View {
    @Binding var text: String
    var prompt = "find a note, name, or phrase…"

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass").foregroundStyle(Theme.muted).accessibilityHidden(true)
            TextField(prompt, text: $text)
                .font(.uiBody)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.search)
            if !text.isEmpty {
                Button { text = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(Theme.muted) }
                    .accessibilityLabel("Clear search")
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 12)
        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(Theme.border))
    }
}

/// Results for the shared query, with its own empty state. Used by pages that are not Memory.
struct SearchResults: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let results = model.searchResults
        VStack(alignment: .leading, spacing: 12) {
            if model.demo.searchUnavailable {
                Text("couldn't search right now.").font(.brandHeading).foregroundStyle(Theme.ink)
                Text("Your notes are safe. Try again in a moment.").foregroundStyle(Theme.muted)
            } else if results.isEmpty {
                Text("not ringing a bell.").font(.brandHeading).foregroundStyle(Theme.ink)
                Text("No matches. Try a different word.").foregroundStyle(Theme.muted)
            } else {
                Text("\(results.count) \(results.count == 1 ? "match" : "matches")").font(.brandMono).foregroundStyle(Theme.muted)
                ForEach(results) { item in
                    NavigationLink(value: item.route) { ItemRow(item: item) }.buttonStyle(.plain)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
