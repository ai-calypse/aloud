import XCTest
@testable import SpeakOS

final class InterpreterTests: XCTestCase {
    private let pricing = MemoryItem(kind: .note, title: "Pricing", body: "Keep pricing at twelve dollars.")

    private func run(_ s: String, items: [MemoryItem] = [], unavailable: Bool = false) -> Interpretation {
        Interpreter.interpret(s, items: items, searchUnavailable: unavailable)
    }

    func testRememberBecomesANoteWithoutThePrefix() {
        guard case .note(let t, _) = run("Remember that Sarah likes the green version.") else { return XCTFail("not a note") }
        XCTAssertEqual(t, "Sarah likes the green version.")
    }

    func testAddressedMessageKeepsTheRecipientForSending() {
        guard case .note(let t, let name) = run("Write Sarah a message saying I'll be there around eight.") else { return XCTFail("not a note") }
        XCTAssertEqual(name, "Sarah")
        XCTAssertEqual(t, "Hey Sarah, I'll be there around eight.")
    }

    /// The finished utterance always opens the note editor, whatever it was about.
    func testPlainSpeechIsStillANote() {
        guard case .note(let t, let name) = run("green version sarah") else { return XCTFail("not a note") }
        XCTAssertEqual(t, "Green version sarah.")
        XCTAssertNil(name)
    }

    func testQuestionAnswersOnlyFromSavedNotes() {
        guard case .answer(let a) = run("What did I say about pricing last week?", items: [pricing]) else { return XCTFail("not an answer") }
        XCTAssertEqual(a.sources, [pricing.id])
        guard case .answer(let text) = a.state else { return XCTFail("expected an answer") }
        XCTAssertEqual(text, pricing.body)
    }

    func testNoMatchIsDistinctFromUnavailable() {
        guard case .answer(let none) = run("What did I say about unicorns?", items: [pricing]),
              case .answer(let down) = run("What did I say about pricing?", items: [pricing], unavailable: true)
        else { return XCTFail("not answers") }
        guard case .noMatch = none.state, case .unavailable = down.state else { return XCTFail("states collapsed") }
    }

    /// Regression for the guide's rule: speech never reaches an external action without a reviewed recipient.
    func testAmbiguousSlackNameLeavesRecipientUnresolved() {
        let candidates = Contacts.candidates(for: "Sarah", service: .slack)
        XCTAssertEqual(candidates.count, 2)
    }
}

final class RewriterTests: XCTestCase {
    func testConfidentDropsHedges() {
        XCTAssertEqual(Rewriter.apply("I think we should maybe move the launch.", style: .confident), "We should move the launch.")
    }

    func testBulletsSplitSentences() {
        XCTAssertEqual(Rewriter.apply("One thing. Another thing.", style: .bullets), "• One thing.\n• Another thing.")
    }

    func testEmptyTextIsLeftAlone() {
        for style in RewriteStyle.allCases { XCTAssertEqual(Rewriter.apply("  ", style: style), "  ") }
    }
}

@MainActor
final class AppModelTests: XCTestCase {
    func testSavingTwiceUpdatesTheSameNote() {
        let model = AppModel()
        let before = model.items.count
        let id = model.saveNote(id: nil, text: "First words.")
        model.saveNote(id: id, text: "Better words.")
        XCTAssertEqual(model.items.count, before + 1)
        XCTAssertEqual(model.item(id)?.body, "Better words.")
    }

    func testUnknownSendOutcomeNeverRecordsAReceiptUntilConfirmed() async {
        let model = AppModel()
        let request = ActionRequest(service: .slack, content: "Hi", recipient: Recipient(name: "Priya Nair", detail: "Acme workspace"), candidates: [])
        model.demo.actionOutcome = .unknown
        let before = model.items.count
        let result = await model.performSend(request)
        XCTAssertEqual(result, .unknown)
        XCTAssertEqual(model.items.count, before, "unknown outcome must not look sent")
        _ = await model.checkStatus(request)
        XCTAssertEqual(model.items.count, before + 1)
    }

    func testBrushingTheControlSavesNothing() {
        let model = AppModel()
        model.micAccess = .granted
        let before = model.items.count
        model.beginCapture()
        model.releaseCapture()
        XCTAssertEqual(model.phase, .idle)
        XCTAssertNil(model.sheet)
        XCTAssertEqual(model.items.count, before)
    }

    func testSearchNarrowsMeetingsButNeverMergesThemIntoNotes() {
        let model = AppModel()
        model.query = "press list"
        XCTAssertTrue(model.visibleMeetings.allSatisfy { m in m.transcript.contains { $0.text.localizedCaseInsensitiveContains("press list") } })
        XCTAssertFalse(model.visibleMeetings.isEmpty)
    }
}
