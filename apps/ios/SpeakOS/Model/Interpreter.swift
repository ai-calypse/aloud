import Foundation

enum Interpretation {
    case note(text: String, recipientName: String?)
    case answer(AnswerModel)
}

/// Stands in for the intent model. After you speak, the result is a note you can rewrite and
/// send; the only other outcome is a question, which is answered from saved notes. Rules, not
/// language understanding: the prototype only needs both outcomes to be believable.
enum Interpreter {
    static func interpret(_ raw: String, items: [MemoryItem], searchUnavailable: Bool) -> Interpretation {
        let text = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        let lower = text.lowercased()

        let questionStarts = ["what ", "when ", "where ", "who ", "how ", "which ", "did i", "do i", "have i", "find ", "search "]
        if text.hasSuffix("?") || questionStarts.contains(where: { lower.hasPrefix($0) }) {
            return .answer(answer(to: text, items: items, searchUnavailable: searchUnavailable))
        }

        if let rest = strip(lower, text, prefixes: ["remember that ", "remember to ", "remember ", "note that "]) {
            return .note(text: sentence(rest), recipientName: nil)
        }

        // "Write Sarah a message saying …" and "Send Sarah a Slack saying …" both become the words to send.
        let addressing = ["write ", "draft ", "compose ", "send ", "email ", "message ", "tell ", "slack ", "dm "]
        if addressing.contains(where: { lower.hasPrefix($0) }) {
            let name = recipientName(in: text)
            let body = payload(in: text).map(sentence)
            if let name, let body { return .note(text: "Hey \(name), \(body)", recipientName: name) }
            return .note(text: body ?? sentence(text), recipientName: name)
        }
        return .note(text: sentence(text), recipientName: nil)
    }

    // MARK: Answers

    private static let stopwords: Set<String> = [
        "what", "when", "where", "which", "about", "said", "last", "week", "did", "does", "have", "that",
        "this", "with", "from", "your", "tell", "find", "search", "there", "anything", "something", "say",
    ]

    static func answer(to question: String, items: [MemoryItem], searchUnavailable: Bool) -> AnswerModel {
        if searchUnavailable { return AnswerModel(question: question, state: .unavailable) }
        let keywords = words(question.lowercased()).filter { $0.count >= 4 && !stopwords.contains($0) }
        let hits = keywords.isEmpty ? [] : items.filter { item in
            let haystack = (item.title + " " + item.body).lowercased()
            return keywords.contains { haystack.contains($0) }
        }
        guard !hits.isEmpty else { return AnswerModel(question: question, state: .noMatch) }
        let used = Array(hits.prefix(3))
        // The answer only quotes saved material, so it can never claim more than the sources say.
        return AnswerModel(question: question, state: .answer(used.map(\.body).joined(separator: " ")), sources: used.map(\.id))
    }

    // MARK: Parsing helpers

    static func words(_ s: String) -> [String] {
        s.components(separatedBy: CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "'")).inverted).filter { !$0.isEmpty }
    }

    static func strip(_ lower: String, _ original: String, prefixes: [String]) -> String? {
        for p in prefixes where lower.hasPrefix(p) { return String(original.dropFirst(p.count)) }
        return nil
    }

    static func service(in lower: String) -> ServiceKind? {
        if lower.contains("slack") { return .slack }
        if lower.contains("gmail") || lower.contains("email") || lower.contains("mail") { return .gmail }
        if lower.contains("notion") { return .notion }
        return nil
    }

    static func payload(in text: String) -> String? {
        for marker in [" saying that ", " saying ", " that says ", " that "] {
            if let r = text.range(of: marker, options: .caseInsensitive) {
                let tail = String(text[r.upperBound...]).trimmingCharacters(in: .whitespaces)
                if !tail.isEmpty { return tail }
            }
        }
        return nil
    }

    static func recipientName(in text: String) -> String? {
        let tokens = text.split(separator: " ").map { $0.trimmingCharacters(in: .punctuationCharacters) }
        let verbs: Set<String> = ["send", "message", "email", "tell", "text", "dm", "slack", "write", "ping", "post", "draft"]
        let skip: Set<String> = ["a", "an", "the", "this", "it", "my", "to", "slack", "gmail", "email", "notion", "message", "note", "draft", "mail"]
        let stop: Set<String> = ["saying", "that"]
        guard let start = tokens.firstIndex(where: { verbs.contains($0.lowercased()) }) else { return nil }
        for token in tokens[(start + 1)...] {
            let l = token.lowercased()
            if stop.contains(l) { return nil }
            if token.isEmpty || skip.contains(l) { continue }
            return token.capitalized
        }
        return nil
    }

    static func sentence(_ s: String) -> String {
        var t = s.trimmingCharacters(in: .whitespacesAndNewlines).trimmingCharacters(in: CharacterSet(charactersIn: "\"“”"))
        guard let first = t.first else { return t }
        t = first.uppercased() + t.dropFirst()
        if let last = t.last, !".!?".contains(last) { t += "." }
        return t
    }
}

enum Contacts {
    static func candidates(for name: String?, service: ServiceKind) -> [Recipient] {
        switch service {
        case .notion:
            return [Recipient(name: "Aloud inbox", detail: "Notion · Personal workspace")]
        case .slack:
            let all = [
                Recipient(name: "Sarah Chen", detail: "Acme workspace"),
                Recipient(name: "Sarah Okafor", detail: "Design Guild workspace"),
                Recipient(name: "Priya Nair", detail: "Acme workspace"),
            ]
            return filter(all, name: name)
        case .gmail:
            let all = [
                Recipient(name: "Sarah Chen", detail: "sarah@acme.co"),
                Recipient(name: "Priya Nair", detail: "priya@acme.co"),
            ]
            return filter(all, name: name)
        }
    }

    /// No name means we cannot guess, so the person picks. An unknown name still
    /// resolves to one candidate they confirm on the preview, never silently.
    private static func filter(_ all: [Recipient], name: String?) -> [Recipient] {
        guard let name else { return all }
        let hits = all.filter { $0.name.localizedCaseInsensitiveContains(name) }
        return hits.isEmpty ? [Recipient(name: name, detail: "Not in your contacts")] : hits
    }
}

enum DemoScript {
    static let samples = [
        "Remember that Sarah likes the green version.",
        "Write Sarah a message saying I'll be there around eight.",
        "I think we should maybe just move the launch to the fourteenth, it's really the best date.",
        "What did I say about pricing last week?",
    ]
    static let sampleLabels = ["Note", "Note for Sarah", "Rambling note (try rewrites)", "Memory question"]

    static let meetingLines = [
        "Let's lock the launch for the 14th.",
        "We agreed to keep the price at twelve dollars a month.",
        "Priya will send the press list on Friday.",
        "We still need to decide on the yearly plan.",
        "Sarah wants the green version for the store page.",
    ]
}
