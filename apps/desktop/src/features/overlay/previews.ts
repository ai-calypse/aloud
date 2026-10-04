import type { Run } from "@/lib/agent-steps";

/**
 * A dictation preview, as whisper produces one: each entry is the *whole*
 * transcript at that moment, so it can go backwards. The two shortenings are
 * deliberate — a revision is what makes the panel shrink mid-flight.
 */
export const PARTIAL_PREVIEW = [
  "So the",
  "So the thing about",
  "So the thing about springs is",
  "So the thing about a spring is that",
  "So the thing about a spring is that you can redirect it",
  "So the thing about a spring is that you can redirect it mid flight and it",
  // Revised shorter again, the case that actually shrinks the panel.
  "So the thing about a spring is that you can redirect it mid-flight",
  "So the thing about a spring is that you can redirect it mid-flight and it keeps the velocity it already had, which is the whole reason",
  "So the thing about a spring is that you can redirect it mid-flight and it keeps the velocity it already had, which is the whole reason it does not stall halfway.",
];

/** How fast `partial: "stream"` advances, in ms between revisions. */
export const PARTIAL_MS = 320;

/** A canned agent run for the tuning page; see PreviewCommand. */
export const AGENT_PREVIEW: Run = {
  heard: "Send Priya the release notes and draft a reply to Sam",
  steps: [
    {
      name: "GMAIL_SEND_EMAIL",
      app: "Gmail",
      args: { recipient_email: "priya@example.com", subject: "Release notes" },
      requiresConfirmation: true,
      confirmed: true,
      result: { success: true, message: "Sent", show: null },
    },
    {
      name: "GMAIL_CREATE_EMAIL_DRAFT",
      app: "Gmail",
      args: { recipient_email: "sam@example.com", subject: "Re: Thursday" },
      requiresConfirmation: true,
    },
  ],
};

const slackText = "Standup moves to 10:30 tomorrow.\nSame link as always.";

/**
 * A write no hand view draws, so its gate shows the schema-built card. The
 * preview is what `aloud_agent::preview::build` makes of these arguments
 * against Slack's live chat.postMessage schema (the Rust test fixture).
 */
export const FALLBACK_PREVIEW: Run = {
  heard: "Tell general on Slack that standup moves to 10:30",
  steps: [
    {
      name: "SLACK_CHAT_POST_MESSAGE",
      app: "slack",
      args: { channel: "general", markdown_text: slackText },
      requiresConfirmation: true,
      preview: {
        kind: "message",
        tier: "confirm",
        verb: "Send",
        title: "Send message to general",
        fields: [
          {
            role: "destination",
            path: "/channel",
            label: "Channel",
            value: "general",
            items: [],
            count: 0,
            name: null,
            unresolved: false,
            before: null,
            origin: null,
            editable: false,
          },
          {
            role: "text",
            path: "/markdown_text",
            label: "Markdown Text",
            value: slackText,
            items: [],
            count: 0,
            name: null,
            unresolved: false,
            before: null,
            origin: null,
            editable: false,
          },
        ],
        more: [],
        set: 2,
        total: 12,
        risks: [],
        pickedBy: null,
        recoverable: null,
        safer: null,
        autoMs: null,
        revision: 0,
        scope: null,
      },
    },
  ],
};
