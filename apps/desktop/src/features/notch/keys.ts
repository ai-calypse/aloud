/** What a keystroke asks of the notch shell. */
export type ShellIntent =
  | { kind: "section"; digit: number }
  | { kind: "sub"; delta: 1 | -1 }
  | { kind: "palette" }
  | null;

/** The keystroke, and where focus was when it landed. */
export type ShellKey = {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  /** Focus is in a text field: its keys are its own. */
  editing: boolean;
  /** Focus is inside the body (a card, a list): Tab moves between its controls. */
  inBody: boolean;
};

/**
 * The shell's keyboard, as a pure decision so it is testable without a DOM.
 *
 * ⌘1–9 and ⌘K work from anywhere, a text field included — they are chords no
 * field types. Tab and Shift-Tab step through subsections only from the
 * panel's chrome or from nowhere; inside the body they are ordinary focus
 * movement, or an approval card's buttons could not be reached by keyboard.
 * Escape is not here: closing is the overlay's, and the palette and the cards
 * catch theirs before it.
 */
export function shellIntent(k: ShellKey): ShellIntent {
  const command = (k.metaKey || k.ctrlKey) && !k.altKey;
  if (command && /^[1-9]$/.test(k.key)) return { kind: "section", digit: Number(k.key) };
  if (command && k.key.toLowerCase() === "k") return { kind: "palette" };
  if (k.key === "Tab" && !command && !k.altKey && !k.editing && !k.inBody) {
    return { kind: "sub", delta: k.shiftKey ? -1 : 1 };
  }
  return null;
}
