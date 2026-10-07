import XCTest
@testable import SpeakOS

final class InterpreterTests: XCTestCase {
    private let pricing = MemoryItem(kind: .thought, title: "Pricing", body: "Keep pricing at twelve dollars.")

    private func run(_ s: String, items: [MemoryItem] = [], unavailable: Bool = false) -> Interpretation {
        Interpreter.interpret(s, items: items, searchUnavailable: unavailable)
    }

    func testRememberBecomesThoughtWithoutPrefix() {
        guard case .thought(let t) = run("Remember that Sarah likes the green version.") else { return XCTFail("not a thought") }
        XCTAssertEqual(t, "Sarah likes the green version.")
    }

    /// Regression for the guide's rule: speech never reaches an external action without a reviewed recipient.
    func testAmbiguousSlackNameLeavesRecipientUnresolved() {
        guard case .action(let r) = run("Send Sarah a Slack saying I'll be there around eight.") else { return XCTFail("not an action") }
        XCTAssertEqual(r.service, .slack)
        XCTAssertNil(r.recipient)
        XCTAssertEqual(r.candidates.count, 2)
        XCTAssertEqual(r.content, "I'll be there around eight.")
    }

    func testUniqueEmailRecipientIsResolved() {
        guard case .action(let r) = run("Email Priya saying the deck is ready") else { return XCTFail("not an action") }
        XCTAssertEqual(r.recipient?.name, "Priya Nair")
    }

    func testDraftIsNotAnAction() {
        guard case .draft(let text, let name) = run("Write Sarah a message saying I'll be there around eight.") else { return XCTFail("not a draft") }
        XCTAssertEqual(name, "Sarah")
        XCTAssertEqual(text, "Hey Sarah, I'll be there around eight.")
    }

    func testQuestionAnswersOnlyFromSavedMemories() {
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

    func testUnrecognisedTextAsksInsteadOfGuessing() {
        guard case .unclear = run("green version sarah") else { return XCTFail("should ask") }
    }
}

@MainActor
final class AppModelTests: XCTestCase {
    func testThoughtCanBeUndone() {
        let model = AppModel()
        let before = model.items.count
        model.saveThought("Sarah likes the green version.")
        XCTAssertEqual(model.items.count, before + 1)
        model.undoLastSaved()
        XCTAssertEqual(model.items.count, before)
        XCTAssertNil(model.lastSaved)
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
        XCTAssertEqual(model.items.count, before)
    }
}
