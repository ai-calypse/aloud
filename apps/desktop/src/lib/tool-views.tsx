import type { ReactNode } from "react";

/**
 * What the agent panel draws for each tool.
 *
 * The panel does not hide fields — it chooses what to draw. `user_id: "me"`
 * and `is_html: false` never appear because nothing here references them.
 * Presentation belongs to the app, not to Composio, which only supplies
 * schemas and executes calls.
 *
 * A registry rather than a switch: aloud can now connect any of Composio's
 * 800+ toolkits, so adding a service's custom UI should be registering one
 * entry, not extending a growing conditional. Everything unregistered falls
 * back to a humanised label and no body, which is the honest default for a
 * tool we know nothing about.
 */

/** A tool call's arguments. Unknown shape — every read must be defensive. */
export type ToolArgs = Record<string, unknown>;

export type ToolView = {
  /** Present tense, what is happening: "Drafting an email to Ana". */
  label: (args: ToolArgs) => string;
  /** The one thing worth showing. Omitted means the label says it all. */
  body?: (args: ToolArgs) => ReactNode;
};

/** A string field, or "" when absent or not a string. */
function text(args: ToolArgs, key: string): string {
  const value = args[key];
  return typeof value === "string" ? value : "";
}

/**
 * The panel-window card every tool body sits in. Local to this file rather
 * than a components/ui primitive: the white/10 hairline is the floating
 * panel's material, not the settings window's.
 */
function ToolCard({ children }: { children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1 rounded-lg border border-white/10 bg-white/5 p-3">
      {children}
    </div>
  );
}

/** "someone@example.com" → "someone", for a label that fits one line. */
function who(address: string): string {
  return address.split("@")[0] || address;
}

/** The email body, which is the whole point of a draft. */
function emailBody(args: ToolArgs): ReactNode {
  const body = text(args, "body");
  const subject = text(args, "subject");
  if (!body && !subject) return null;

  return (
    <ToolCard>
      {subject && <p className="m-0 text-ui font-medium">{subject}</p>}
      {body ? (
        <p className="m-0 whitespace-pre-wrap text-ui leading-relaxed text-muted-foreground">
          {body}
        </p>
      ) : (
        // Surfaced rather than silently sent: an empty body reaching the
        // confirmation is the model having dropped the content.
        <p className="m-0 text-ui italic text-warning">
          No message body — cancel and ask again.
        </p>
      )}
    </ToolCard>
  );
}

/** When and where, which is what you check before letting an event save. */
function eventBody(args: ToolArgs): ReactNode {
  const start = text(args, "start_datetime");
  const location = text(args, "location");
  if (!start && !location) return null;

  return (
    <ToolCard>
      {start && (
        <p className="m-0 text-ui font-medium">{start.replace("T", " ")}</p>
      )}
      {location && (
        <p className="m-0 text-ui text-muted-foreground">{location}</p>
      )}
    </ToolCard>
  );
}

const VIEWS: Record<string, ToolView> = {
  GMAIL_SEND_EMAIL: {
    label: (a) => {
      const to = text(a, "recipient_email");
      return to ? `Sending an email to ${who(to)}` : "Sending an email";
    },
    body: emailBody,
  },
  GMAIL_CREATE_EMAIL_DRAFT: {
    label: (a) => {
      const to = text(a, "recipient_email");
      return to ? `Drafting an email to ${who(to)}` : "Drafting an email";
    },
    body: emailBody,
  },
  GMAIL_FETCH_EMAILS: { label: () => "Reading your inbox" },
  GMAIL_LIST_DRAFTS: { label: () => "Looking through your drafts" },

  GITHUB_CREATE_AN_ISSUE: {
    label: (a) => {
      const repo = text(a, "repo");
      return repo ? `Opening an issue on ${repo}` : "Opening an issue";
    },
    body: (a) => {
      const title = text(a, "title");
      const body = text(a, "body");
      if (!title) return null;
      return (
        <ToolCard>
          <p className="m-0 text-ui font-medium">{title}</p>
          {body && (
            <p className="m-0 whitespace-pre-wrap text-ui leading-relaxed text-muted-foreground">
              {body}
            </p>
          )}
        </ToolCard>
      );
    },
  },
  GITHUB_LIST_REPOSITORY_ISSUES: {
    label: (a) => {
      const repo = text(a, "repo");
      return repo ? `Reading issues on ${repo}` : "Reading issues";
    },
  },
  GITHUB_GET_THE_AUTHENTICATED_USER: { label: () => "Checking your GitHub account" },

  GOOGLECALENDAR_CREATE_EVENT: {
    label: (a) => {
      const title = text(a, "summary");
      return title ? `Adding "${title}" to your calendar` : "Adding a calendar event";
    },
    body: eventBody,
  },
  GOOGLECALENDAR_UPDATE_EVENT: {
    label: (a) => {
      const title = text(a, "summary");
      return title ? `Updating "${title}"` : "Updating a calendar event";
    },
    body: eventBody,
  },
  GOOGLECALENDAR_FIND_EVENT: { label: () => "Looking through your calendar" },
  GOOGLECALENDAR_FIND_FREE_SLOTS: { label: () => "Finding a free slot" },
  // The only destructive tool in the default set, and it was the one with no
  // view: its confirmation read "Googlecalendar: delete event" over an empty
  // body. Delete carries no title or time — only ids — so the gate shows the
  // id rather than pretending to more.
  GOOGLECALENDAR_DELETE_EVENT: {
    label: () => "Deleting a calendar event",
    body: (args) => {
      const id = text(args, "event_id");
      const calendar = text(args, "calendar_id");
      if (!id) return null;
      return (
        <ToolCard>
          <p className="m-0 text-ui font-medium">Event {id}</p>
          {calendar && calendar !== "primary" && (
            <p className="m-0 text-ui text-muted-foreground">{calendar}</p>
          )}
        </ToolCard>
      );
    },
  },
};

/**
 * "SLACK_SEND_MESSAGE" → "Slack: send message".
 *
 * The fallback for the toolkits nobody has written a view for, which after
 * the Connectors work is most of them.
 */
export function humanise(slug: string): string {
  const [toolkit, ...rest] = slug.split("_");
  if (rest.length === 0) return slug;
  const app = toolkit.charAt(0) + toolkit.slice(1).toLowerCase();
  return `${app}: ${rest.join(" ").toLowerCase()}`;
}

export function viewFor(name: string): ToolView {
  return VIEWS[name] ?? { label: () => humanise(name) };
}

/** What the panel prints for a call — the registered label, or the fallback. */
export function labelFor(name: string, args: ToolArgs): string {
  try {
    return viewFor(name).label(args);
  } catch {
    // A malformed argument object must never blank the panel.
    return humanise(name);
  }
}

export function bodyFor(name: string, args: ToolArgs): ReactNode {
  try {
    return viewFor(name).body?.(args) ?? null;
  } catch {
    return null;
  }
}
