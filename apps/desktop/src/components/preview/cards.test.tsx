import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { ActionPreview, CardKind } from "@/lib/bindings";
import { CARD_PREVIEWS } from "@/features/overlay/previews";
import { PreviewCard, layoutFor } from "./cards";

const card = (kind: CardKind) =>
  CARD_PREVIEWS[kind].steps[0].preview as ActionPreview;
const html = (kind: CardKind, compact = false) =>
  renderToStaticMarkup(<PreviewCard preview={card(kind)} compact={compact} />);

describe("PreviewCard", () => {
  it("draws kinds that read alike with one layout, and every other kind its own", () => {
    expect(layoutFor("email")).toBe(layoutFor("message"));
    expect(layoutFor("note")).toBe(layoutFor("task"));
    expect(layoutFor("row")).toBe(layoutFor("issue"));
    expect(layoutFor("share")).toBe(layoutFor("issue"));
    const own: CardKind[] = [
      "message",
      "task",
      "issue",
      "event",
      "file",
      "update",
      "delete",
      "money",
      "fallback",
    ];
    expect(new Set(own.map(layoutFor)).size).toBe(own.length);
  });

  it("leads each card with what its kind is about", () => {
    expect(html("event")).toContain("preview__when--big");
    expect(html("money")).toContain(
      '<p class="preview__lead"><span class="preview__money">42.00 USD</span>',
    );
    expect(html("update")).toContain('<span class="preview__old">Lunch</span>');
    expect(html("file")).toContain(
      "~/Downloads/tax-2025.pdf<br/>→ ~/Documents/Taxes",
    );
    const del = html("delete");
    expect(del).toContain("Matched: category:promotions older_than:30d");
    expect(del).toContain("and 35 more");
  });

  it("says whose words a message is", () => {
    expect(html("message")).toContain("your words");
    expect(html("email")).toContain("written by Aloud");
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
