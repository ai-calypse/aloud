import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { ActionPreview, CardKind } from "@/lib/bindings";
import { CARD_PREVIEWS } from "@/features/overlay/previews";
import { PreviewCard, layoutFor } from "./cards";
import { CardEditsContext } from "./kit";

const KINDS = Object.keys(CARD_PREVIEWS) as CardKind[];
const step = (kind: CardKind) => CARD_PREVIEWS[kind].steps[0];
const card = (kind: CardKind) => step(kind).preview as ActionPreview;
const html = (kind: CardKind, compact = false) =>
  renderToStaticMarkup(
    <PreviewCard preview={card(kind)} app={step(kind).app} compact={compact} />,
  );
/** The window's title bar, as rendered. */
const title = (kind: CardKind) =>
  /<span class="preview__title">(.*?)<\/span><\/div>/.exec(html(kind))?.[1];

describe("PreviewCard", () => {
  /** Every kind is its own app window; none borrows another's arrangement. */
  it("draws each kind as its own window", () => {
    expect(new Set(KINDS.map((k) => layoutFor(k).body)).size).toBe(KINDS.length);
  });

  it("titles each window the way its app does", () => {
    expect(title("email")).toBe("New Message");
    expect(title("message")).toBe('<span class="preview__sigil">#</span>launch');
    expect(title("event")).toBe("New Event");
    expect(title("task")).toBe("New Task");
    expect(title("note")).toBe("New Note");
    expect(title("issue")).toBe("New Issue");
    // Kinds whose app has no window of their own say what they do.
    expect(title("row")).toBe("Add row");
    expect(title("money")).toBe("Charge Acme Inc");
    expect(layoutFor("task").title(card("task"), "Reminders")).toBe("New Reminder");
    // A channel not yet looked up is flagged in the title, never shown as a name.
    const unnamed = {
      ...card("message"),
      fields: [{ ...card("message").fields[0], name: null, unresolved: true }],
    };
    expect(
      renderToStaticMarkup(<PreviewCard preview={unnamed} app="Slack" />),
    ).toContain("not looked up");
  });

  it("wears the acting app's icon, or a neutral glyph without one", () => {
    const icon = "data:image/png;base64,AA";
    const withIcon = renderToStaticMarkup(
      <PreviewCard preview={card("email")} app="Gmail" icon={icon} />,
    );
    expect(withIcon).toContain(`<img class="app-icon" src="${icon}" alt="Gmail"`);
    expect(html("email")).toContain('class="app-icon app-icon--glyph"');
  });

  /** Mockup 1: a To chip on a hairline, the subject plain, then the body. */
  it("lays an email out as a compose window", () => {
    const email = html("email");
    expect(email).toContain('<span class="preview__label">To</span>');
    expect(email).toContain('<span class="preview__chip">david@company.com</span>');
    expect(email).toContain(
      '<p class="preview__lead">Project update and Thursday sync</p>',
    );
    expect(email.indexOf("david@company.com")).toBeLessThan(email.indexOf("Hi David"));
    expect(email).not.toContain(">Subject<");
  });

  it("puts a chat message in a bubble and leads each other card with what its kind is about", () => {
    expect(html("message")).toContain(
      '<div class="preview__bubble"><p class="preview__body">hi</p></div>',
    );
    expect(html("event")).toContain("preview__when--big");
    expect(html("task")).toContain('<span class="preview__check" aria-hidden="true">');
    expect(html("task")).toContain('<span class="preview__due">Sat, Oct 3, 2026, 5:00 PM</span>');
    expect(html("issue")).toContain('<span class="preview__faint">Priority</span>High');
    expect(html("money")).toContain(
      '<p class="preview__lead"><span class="preview__money">42.00 USD</span>',
    );
    expect(html("update")).toContain('<span class="preview__old">Lunch</span>');
    const file = html("file");
    expect(file).toContain('<span class="preview__path">~/Downloads/tax-2025.pdf</span>');
    expect(file.indexOf("tax-2025.pdf")).toBeLessThan(file.indexOf("~/Documents/Taxes"));
    expect(html("share")).toContain("Q4 Deck");
    expect(html("share")).toContain('<span class="preview__faint">Access</span>');
    const del = html("delete");
    expect(del).toContain("Matched: category:promotions older_than:30d");
    expect(del).toContain("and 35 more");
  });

  it("draws a sheet's rows as cells under their columns, and an event's zone under its time", () => {
    const row = html("row");
    expect(row).toContain("<th>A</th><th>B</th><th>C</th><th>D</th>");
    expect(row).toContain("<td>3 Oct</td><td>Lunch</td><td>42</td><td>Food</td>");
    expect(html("event")).toContain(
      '<span class="preview__under">Asia/Kolkata · 1 hr</span>',
    );
  });

  /** A layout only arranges: every value Rust sent is somewhere on its card. */
  it("drops no field from any window", () => {
    for (const kind of KINDS) {
      const drawn = html(kind);
      for (const field of card(kind).fields) {
        // A sheet's rows are drawn cell by cell, and a list item by item,
        // rather than as the joined line.
        const shown = field.cells.length
          ? [field.cells[1].map((cell) => `<td>${cell}</td>`).join("")]
          : field.items.length
            ? field.items.map((item) => item.split(" · ").pop() ?? item)
            : [(field.name ?? field.value).split("\n")[0]];
        for (const piece of shown) {
          expect(drawn, `${kind} ${field.path}`).toContain(piece.replace(/'/g, "&#x27;"));
        }
      }
    }
  });

  it("names the safer tool a strict delete could use instead, and marks the window strict", () => {
    expect(html("delete")).toContain("Safer: Move to Trash");
    expect(html("delete")).toContain('data-strict="true"');
    expect(html("delete", true)).not.toContain("Safer");
    expect(html("email")).not.toContain("data-strict");
  });

  it("says whose words a message is", () => {
    expect(html("message")).toContain("your words");
    expect(html("email")).toContain("written by Aloud");
  });

  it("puts the gate's answers at the window's foot", () => {
    const open = renderToStaticMarkup(
      <PreviewCard preview={card("email")} actions={<button className="app-pill">Send</button>} />,
    );
    expect(open).toMatch(
      /<div class="preview__actions"><button class="app-pill">Send<\/button><\/div><\/div>$/,
    );
    expect(html("email")).not.toContain("preview__actions");
  });

  /**
   * Only the card waiting on a yes takes edits, and only in the fields Rust
   * offered: the finished echo of the same card, or its channel, stays text.
   */
  it("edits the open card's offered fields in place, and nothing else", () => {
    const draft = { "/markdown_text": "hey team" };
    const open = renderToStaticMarkup(
      <CardEditsContext.Provider value={{ draft, edit: () => {}, hold: () => {} }}>
        <PreviewCard preview={card("message")} app="Slack" />
      </CardEditsContext.Provider>,
    );
    expect(open).toContain("<textarea");
    expect(open).toContain("hey team");
    expect(open).not.toContain("<input");
    expect(open).toContain("launch");

    const echo = html("message", true);
    expect(echo).not.toContain("<textarea");
  });

  /** A recipient being corrected is still a chip, not a bare text box. */
  it("keeps an editable recipient a chip", () => {
    const person = { ...card("email").fields[0], editable: true };
    const editable = { ...card("email"), fields: [person, ...card("email").fields.slice(1)] };
    const open = renderToStaticMarkup(
      <CardEditsContext.Provider value={{ draft: {}, edit: () => {}, hold: () => {} }}>
        <PreviewCard preview={editable} />
      </CardEditsContext.Provider>,
    );
    expect(open).toMatch(/<input[^>]*class="[^"]*preview__input--chip/);
  });

  it("echoes a finished step as the window's bar and the one line that names it", () => {
    expect(html("email", true)).toContain(
      '<p class="preview__key">Project update and Thursday sync</p>',
    );
    expect(html("message", true)).toContain('<p class="preview__key">hi</p>');
    expect(html("money", true)).toContain('<p class="preview__key">42.00 USD</p>');
    expect(html("share", true)).toContain('<p class="preview__key">Q4 Deck</p>');
    expect(html("email", true)).toContain("New Message");
  });

  it("keeps warnings, folded fields and the schema count off the compact echo", () => {
    expect(html("delete")).toContain("preview__risk preview__risk--danger");
    expect(html("issue")).toContain('<p class="preview__risk">');
    expect(html("delete", true)).not.toContain("preview__risk");
    expect(html("row")).toContain("preview__more");
    expect(html("row", true)).not.toContain("preview__more");
    expect(html("fallback")).toContain("6 of 8 fields set");
    expect(html("fallback", true)).not.toContain("fields set");
  });
});
