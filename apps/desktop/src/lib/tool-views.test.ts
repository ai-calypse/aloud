import { describe, expect, it } from "vitest";
import { asksFirst, bodyFor, humanise, labelFor, viewFor } from "./tool-views";

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

  /**
   * The reported bug: asked to mail "Charlie yang", the agent took an
   * unrelated person a search happened to return and wrote to them. The gate
   * showed "Sending an email to nadege.guillaumat" — the local part only —
   * and the body card showed subject and message but never the address, so
   * there was nowhere on screen that said who it was really going to.
   */
  describe("the recipient of a send", () => {
    it("shows the whole address, domain included", () => {
      const body = bodyFor("GMAIL_SEND_EMAIL", {
        recipient_email: "nadege.guillaumat@example.com",
        subject: "hi",
        body: "hi",
      });
      expect(JSON.stringify(body)).toContain("nadege.guillaumat@example.com");
    });

    it("says so loudly when a send has no recipient at all", () => {
      const body = bodyFor("GMAIL_SEND_EMAIL", { subject: "hi", body: "hi" });
      expect(JSON.stringify(body)).toContain("No recipient");
    });
  });

  it("names who a directory lookup is looking for", () => {
    expect(labelFor("GMAIL_SEARCH_PEOPLE", { query: "Charlie yang" })).toBe(
      "Looking for Charlie yang",
    );
    expect(labelFor("GOOGLECONTACTS_SEARCH_CONTACTS", {})).toBe(
      "Looking through your contacts",
    );
  });

  /**
   * The People API nests names and addresses in arrays of objects rather than
   * the flat strings every other tool here uses, so this reader digs two levels
   * and has two levels to get wrong. The gate that approves writing a contact
   * shows what these return.
   */
  describe("saving a contact", () => {
    const dad = {
      names: [{ givenName: "Dad", displayName: "Dad" }],
      emailAddresses: [{ value: "dipteshg54@gmail.com" }],
      personFields: "names,emailAddresses",
    };

    it("reads the name out of the nested arrays", () => {
      expect(labelFor("GOOGLECONTACTS_CREATE_CONTACT", dad)).toBe(
        "Saving Dad to your contacts",
      );
    });

    /**
     * The ask-first flow is the agent offering to save someone it could not
     * find. A text with an editable recipient used to open on "Save contact".
     */
    it("is the only editable gate that asks before showing its fields", () => {
      expect(asksFirst("GOOGLECONTACTS_CREATE_CONTACT", dad)).toBe(true);
      expect(asksFirst("ALOUD_MESSAGES_SEND", { to: "+15551234567", text: "hi" })).toBe(
        false,
      );
      expect(asksFirst("ALOUD_DISCORD_MESSAGE", { to: "sam", text: "hi" })).toBe(false);
    });

    /**
     * The shipped bug: `display_args` in agent.rs stringifies every argument
     * that is not already a string, so the panel receives `names` as JSON text.
     * Read as an array it was always empty, and the gate for a contact write
     * said "Saving a contact" with no name and no address under it.
     */
    it("reads them when the panel receives them stringified, as it really does", () => {
      const flattened = {
        names: JSON.stringify(dad.names),
        emailAddresses: JSON.stringify(dad.emailAddresses),
        personFields: "names,emailAddresses",
      };
      expect(labelFor("GOOGLECONTACTS_CREATE_CONTACT", flattened)).toBe(
        "Saving Dad to your contacts",
      );
    });

    it("falls back to given and family names when there is no displayName", () => {
      expect(
        labelFor("GOOGLECONTACTS_CREATE_CONTACT", {
          names: [{ givenName: "Ada", familyName: "Lovelace" }],
        }),
      ).toBe("Saving Ada Lovelace to your contacts");
    });

    it("does not blank on an empty, wrongly-typed or empty-object array", () => {
      expect(labelFor("GOOGLECONTACTS_CREATE_CONTACT", {})).toBe("Saving a contact");
      expect(labelFor("GOOGLECONTACTS_CREATE_CONTACT", { names: "Dad" })).toBe(
        "Saving a contact",
      );
      expect(labelFor("GOOGLECONTACTS_CREATE_CONTACT", { names: [{}] })).toBe(
        "Saving a contact",
      );
      expect(labelFor("GOOGLECONTACTS_CREATE_CONTACT", { names: [null] })).toBe(
        "Saving a contact",
      );
    });
  });
});
