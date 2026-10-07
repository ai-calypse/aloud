import Foundation

enum RewriteStyle: String, CaseIterable, Identifiable {
    // Tone
    case professional, friendly, casual, confident, formal, empathetic
    // Edit
    case fixGrammar, shorter, bullets

    var id: String { rawValue }
    var label: String {
        switch self {
        case .professional: "Professional"
        case .friendly: "Friendly"
        case .casual: "Casual"
        case .confident: "Confident"
        case .formal: "Formal"
        case .empathetic: "Empathetic"
        case .fixGrammar: "Fix grammar"
        case .shorter: "Shorter"
        case .bullets: "Bullet points"
        }
    }
    var isTone: Bool {
        switch self {
        case .fixGrammar, .shorter, .bullets: false
        default: true
        }
    }
    static var tones: [RewriteStyle] { allCases.filter(\.isTone) }
    static var edits: [RewriteStyle] { allCases.filter { !$0.isTone } }
}

/// ponytail: string rules stand in for the rewriting model so the editor's states can be reviewed
/// without one. The upgrade path is one model call per style; the editor only needs text back.
enum Rewriter {
    static func apply(_ text: String, style: RewriteStyle) -> String {
        let clean = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return text }
        switch style {
        case .professional:
            return Interpreter.sentence(expand(removing(clean, ["just ", "kinda ", "sorta ", "like, "])))
        case .friendly:
            let body = clean.lowercased().hasPrefix("hey") ? clean : "Hey! " + clean
            return body + " Thanks so much!"
        case .casual:
            let c = contract(clean)
            return c.hasSuffix(".") ? String(c.dropLast()) : c
        case .confident:
            return Interpreter.sentence(removing(clean, ["I think ", "I guess ", "maybe ", "perhaps ", "I was wondering if ", "kind of ", "sort of ", "just "]))
        case .formal:
            var f = expand(clean)
            f = f.replacingOccurrences(of: "Hey ", with: "Hello ").replacingOccurrences(of: "Thanks", with: "Thank you")
            return f + "\n\nKind regards"
        case .empathetic:
            return "I hear you, and I appreciate you sharing this. " + clean
        case .fixGrammar:
            var g = clean.replacingOccurrences(of: "  ", with: " ").replacingOccurrences(of: " i ", with: " I ").replacingOccurrences(of: " i'", with: " I'")
            g = sentences(g).map(Interpreter.sentence).joined(separator: " ")
            return g
        case .shorter:
            let kept = sentences(removing(clean, ["really ", "very ", "basically ", "actually ", "just "])).prefix(2)
            return kept.map(Interpreter.sentence).joined(separator: " ")
        case .bullets:
            return sentences(clean).map { "• " + Interpreter.sentence($0) }.joined(separator: "\n")
        }
    }

    private static let pairs = [
        ("I'll", "I will"), ("I'm", "I am"), ("I've", "I have"), ("can't", "cannot"), ("won't", "will not"),
        ("don't", "do not"), ("it's", "it is"), ("that's", "that is"), ("we're", "we are"), ("gonna", "going to"),
    ]

    private static func expand(_ s: String) -> String {
        pairs.reduce(s) { $0.replacingOccurrences(of: $1.0, with: $1.1, options: .caseInsensitive) }
    }

    private static func contract(_ s: String) -> String {
        pairs.reduce(s) { $0.replacingOccurrences(of: $1.1, with: $1.0, options: .caseInsensitive) }
    }

    private static func removing(_ s: String, _ phrases: [String]) -> String {
        phrases.reduce(s) { $0.replacingOccurrences(of: $1, with: "", options: .caseInsensitive) }
    }

    private static func sentences(_ s: String) -> [String] {
        s.split(whereSeparator: { ".!?".contains($0) }).map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    }
}
