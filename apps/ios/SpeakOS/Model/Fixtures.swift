import Foundation

/// Returning-user content for the prototype. A real first-use account starts empty
/// (`DemoControls`/Settings can switch to that), so no screen depends on these.
enum Fixtures {
    private static func ago(_ hours: Double) -> Date { Date.now.addingTimeInterval(-hours * 3600) }

    static func meetings() -> [Meeting] {
        let planning = Meeting(
            title: "Launch planning",
            date: ago(26),
            duration: 1_260,
            summary: "Launch is set for the 14th. Pricing stays at twelve dollars a month.",
            decisions: ["Launch on the 14th.", "Keep the price at $12 a month."],
            actionItems: ["Priya sends the press list on Friday."],
            transcript: [
                TranscriptLine(time: 12, text: "Let's lock the launch for the 14th."),
                TranscriptLine(time: 95, text: "We agreed to keep the price at twelve dollars a month."),
                TranscriptLine(time: 310, text: "Priya will send the press list on Friday."),
            ]
        )
        let standup = Meeting(
            title: "Design sync",
            date: ago(75),
            duration: 540,
            summary: "Two icon directions reviewed. Green variant preferred for the store page.",
            decisions: [],
            actionItems: [],
            transcript: [
                TranscriptLine(time: 20, text: "Sarah wants the green version for the store page."),
                TranscriptLine(time: 200, text: "The blue one reads better at small sizes."),
            ]
        )
        let unfinished = Meeting(
            title: "Customer call",
            date: ago(5),
            duration: 724,
            summary: "",
            decisions: [],
            actionItems: [],
            transcript: [
                TranscriptLine(time: 30, text: "They want export to Notion before anything else."),
                TranscriptLine(time: 410, text: "Pricing is fine if the yearly plan is cheaper."),
            ],
            status: .unfinished(stoppedAt: 724)
        )
        return [unfinished, planning, standup]
    }

    static func items(meetings: [Meeting]) -> [MemoryItem] {
        var out: [MemoryItem] = [
            MemoryItem(kind: .thought, title: "Keep pricing at $12 a month", body: "Pricing should stay at twelve dollars a month through launch. Review a yearly option after.", date: ago(30), pinned: true, tags: ["pricing"]),
            MemoryItem(kind: .thought, title: "Ask Priya about the press list", body: "Ask Priya whether the press list includes the podcast people.", date: ago(52), tags: ["launch"]),
            MemoryItem(kind: .draft, title: "Note to the team", body: "Hey team, the launch is on the 14th. Shout if anything blocks you.", date: ago(8)),
            MemoryItem(kind: .action, title: "Sent to Priya Nair on Slack", body: "Can you send the press list by Friday?", date: ago(100)),
        ]
        for m in meetings where m.status == .finished {
            out.append(MemoryItem(kind: .meeting, title: m.title, body: m.summary, date: m.date, meetingID: m.id))
        }
        return out
    }

    static func connections() -> [Connection] {
        [
            Connection(service: .slack, status: .connected, account: "Acme workspace"),
            Connection(service: .gmail, status: .connected, account: "you@example.com"),
            Connection(service: .notion, status: .notConnected, account: ""),
        ]
    }
}
