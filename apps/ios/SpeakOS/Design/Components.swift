import SwiftUI
import UIKit

enum Haptics {
    static func start() { UIImpactFeedbackGenerator(style: .medium).impactOccurred() }
    static func tick() { UIImpactFeedbackGenerator(style: .light).impactOccurred() }
    static func success() { UINotificationFeedbackGenerator().notificationOccurred(.success) }
}

struct Wordmark: View {
    var body: some View {
        Text("speakos")
            .font(.system(.title3, design: .rounded, weight: .heavy))
            .foregroundStyle(Theme.ink)
    }
}

/// Kind label for a memory: icon plus text, so it never relies on colour.
struct KindTag: View {
    let kind: MemoryKind
    var body: some View {
        Label(kind.label, systemImage: kind.symbol)
            .font(.brandMono)
            .foregroundStyle(Theme.primary)
            .labelStyle(.titleAndIcon)
    }
}

struct ItemRow: View {
    let item: MemoryItem
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                KindTag(kind: item.kind)
                if item.pinned { Image(systemName: "pin.fill").font(.caption).foregroundStyle(Theme.muted).accessibilityLabel("pinned") }
                Spacer()
                Text(ago(item.date)).font(.brandMono).foregroundStyle(Theme.muted)
            }
            Text(item.title).font(.headline).foregroundStyle(Theme.ink).multilineTextAlignment(.leading)
            if !item.body.isEmpty, item.body != item.title {
                Text(item.body).font(.subheadline).foregroundStyle(Theme.muted).lineLimit(2).multilineTextAlignment(.leading)
            }
        }
        .card()
        .accessibilityElement(children: .combine)
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
                Rectangle().fill([a, b, c][i] == 1 ? color : .clear).frame(width: cell, height: cell)
            }
        }
    }
}

/// Live input level. Static when reduced motion is on; the label carries the state.
struct Waveform: View {
    var active: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 0.07, paused: !active || reduceMotion)) { context in
            let t = context.date.timeIntervalSinceReferenceDate
            HStack(spacing: 4) {
                ForEach(0..<26, id: \.self) { i in
                    let wave = abs(sin(t * 6 + Double(i) * 0.6)) * abs(cos(Double(i) * 0.9))
                    let level = (active && !reduceMotion) ? 0.15 + 0.85 * wave : 0.25
                    RoundedRectangle(cornerRadius: 2)
                        .fill(active ? Theme.orange : Theme.border)
                        .frame(width: 4, height: 6 + 34 * level)
                }
            }
            .frame(height: 44)
        }
        .accessibilityHidden(true)
    }
}

struct ToastView: View {
    let text: String
    var body: some View {
        Text(text)
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(Theme.ink)
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .background(Theme.yellow, in: Capsule())
            .overlay(Capsule().stroke(Theme.ink.opacity(0.15), lineWidth: 1))
            .padding(.top, 8)
    }
}

/// Small bordered action row used under cards (edit · undo · done).
struct InlineActions<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 8) { content }
            VStack(alignment: .leading, spacing: 8) { content }
        }
        .buttonStyle(.bordered)
        .controlSize(.regular)
    }
}
