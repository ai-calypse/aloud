import { describe, expect, it } from "vitest";
import { humanise, labelFor, viewFor } from "./tool-views";

describe("tool views", () => {
  it("says what the tool is doing, not what it is called", () => {
    expect(
      labelFor("GMAIL_CREATE_EMAIL_DRAFT", {
        recipient_email: "ana@example.com",
        body: "hi",
      }),
    ).toBe("Drafting an email to ana");
    expect(labelFor("GMAIL_FETCH_EMAILS", {})).toBe("Reading your inbox");
  });

  /** After Connectors, most tools come from toolkits nobody wrote a view for. */
  it("falls back to a readable label for an unregistered tool", () => {
    expect(humanise("SLACK_SEND_MESSAGE")).toBe("Slack: send message");
    expect(labelFor("LINEAR_CREATE_ISSUE", {})).toBe("Linear: create issue");
    expect(viewFor("LINEAR_CREATE_ISSUE").body).toBeUndefined();
  });

  /**
   * The reported bug: a draft showed every parameter, including `user_id: me`
   * and `is_html: false`. Nothing references them, so they cannot appear.
   */
  it("never reads the fields a user should not be shown", () => {
    const args = {
      recipient_email: "ana@example.com",
      subject: "Re: hello",
      body: "Thanks!",
      user_id: "me",
      is_html: "false",
    };
    const label = labelFor("GMAIL_SEND_EMAIL", args);
    expect(label).not.toContain("user_id");
    expect(label).not.toContain("is_html");
    expect(label).toBe("Sending an email to ana");
  });

  it("degrades instead of blanking when arguments are missing or malformed", () => {
    expect(labelFor("GMAIL_SEND_EMAIL", {})).toBe("Sending an email");
    expect(labelFor("GMAIL_SEND_EMAIL", { recipient_email: 42 })).toBe(
      "Sending an email",
    );
    expect(labelFor("MYSTERY", {})).toBe("MYSTERY");
  });
});
