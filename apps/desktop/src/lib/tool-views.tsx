import type { ReactNode } from "react";

/**
 * What the agent panel draws for each tool. A registry rather than a switch:
 * any of Composio's 800+ toolkits can be connected, so a service's custom UI
 * is one entry. Anything unregistered falls back to a humanised label, no body.
 */

/** A tool call's arguments. Unknown shape — every read must be defensive. */
export type ToolArgs = Record<string, unknown>;

export type ToolView = {
  /** Present tense, what is happening: "Drafting an email to Ana". */
  label: (args: ToolArgs) => string;
  /** The one thing worth showing. Omitted means the label says it all. */
  body?: (args: ToolArgs) => ReactNode;
  /**
   * Fields the approval gate lets the user fill in before confirming, replacing
   * `body` there. Only worth declaring where the model routinely cannot know
   * the value — an address it never found. Everywhere else an editable gate is
   * a form standing between the user and the thing they asked for.
   */
  edits?: Editable[];
  /**
   * The question the gate asks before showing `edits`. Only meaningful with
   * them: an editable gate is the agent proposing work of its own, so it asks
   * before putting a form in front of someone who never wanted one.
   */
  ask?: (args: ToolArgs) => string;
};

/** One field in an editable approval gate. */
export type Editable = {
  /** Stable across renders; also the React key. */
  key: string;
  label: string;
  placeholder?: string;
  get: (args: ToolArgs) => string;
  /** Returns the whole argument object with this field replaced. */
  set: (args: ToolArgs, value: string) => ToolArgs;
};

/** A string field, or "" when absent or not a string. */
export function text(args: ToolArgs, key: string): string {
  const value = args[key];
  return typeof value === "string" ? value : "";
}

/**
 * The card every tool body sits in. Local rather than a components/ui
 * primitive: the white/10 hairline is the floating panel's material.
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

/** A directory lookup, named for who it is looking for. */
function lookingFor(query: string): string {
  return query ? `Looking for ${query}` : "Looking through your contacts";
}

/** The email body, which is the whole point of a draft. */
function emailBody(args: ToolArgs): ReactNode {
  const body = text(args, "body");
  const subject = text(args, "subject");
  const to = text(args, "recipient_email");
  if (!body && !subject && !to) return null;

  return (
    <ToolCard>
      {/* The whole address, above everything else and never abbreviated. The
          label says "to charlie" whether the address is charlie@… or a stranger
          the lookup happened to return; this is the only place the gate states
          who is actually being written to, so it is the line that has to be
          right. A missing one is worse still — the tool would send to nobody or
          to whatever it defaults to. */}
      {to ? (
        <p className="m-0 break-all text-ui text-muted-foreground">
          To: <span className="text-ink">{to}</span>
        </p>
      ) : (
        <p className="m-0 text-ui italic text-warning">
          No recipient — cancel and ask again.
        </p>
      )}
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
  // The directory lookups. Registered so the step list says who is being
  // looked for — unlabelled they fell through to "Gmail: search people", which
  // reads as plumbing and hides the name the agent actually searched for.
  GMAIL_SEARCH_PEOPLE: {
    label: (a) => lookingFor(text(a, "query")),
  },

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
  // Delete carries no title or time, only ids, so the gate shows the id rather
  // than pretending to more.
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

  GOOGLECONTACTS_SEARCH_CONTACTS: {
    label: (a) => lookingFor(text(a, "query")),
  },
  // The People API takes names and addresses as arrays of objects, not the flat
  // strings every other tool here uses, so both readers dig rather than `text`.
  GOOGLECONTACTS_CREATE_CONTACT: {
    label: (a) => {
      const name = contactName(a);
      return name ? `Saving ${name} to your contacts` : "Saving a contact";
    },
    // A contact you cannot see before approving is a contact you did not agree
    // to — the address is the part that is easy to get wrong.
    body: (a) => {
      const name = contactName(a);
      const email = contactEmail(a);
      if (!name && !email) return null;
      return (
        <ToolCard>
          {name && <p className="m-0 text-ui font-medium">{name}</p>}
          {email && <p className="m-0 text-ui text-muted-foreground">{email}</p>}
        </ToolCard>
      );
    },
    ask: (a) => {
      const name = contactName(a);
      return name
        ? `I could not find ${name}. Save them as a contact?`
        : "Save this person as a contact?";
    },
    // The one gate worth making editable: the agent reaches it precisely when
    // it could not find an address, so the field it most needs is the one field
    // it cannot fill. Both write back the People API's nested shape, stringified
    // the way `display_args` delivers it, so Rust parses it straight back.
    edits: [
      {
        key: "name",
        label: "Name",
        placeholder: "Their name",
        get: contactName,
        set: (args, value) => ({
          ...args,
          names: JSON.stringify([{ givenName: value, displayName: value }]),
        }),
      },
      {
        key: "email",
        label: "Email",
        placeholder: "name@example.com",
        get: contactEmail,
        set: (args, value) => ({
          ...args,
          emailAddresses: JSON.stringify([{ value }]),
        }),
      },
    ],
  },

  // Native macOS actions (aloud-native). Registered for real labels; the
  // fall-through would render "Aloud: music play" and hide what actually runs.
  ALOUD_MUSIC_PLAY: {
    label: (a) => {
      const q = text(a, "query");
      return q ? `Playing ${q}` : "Resuming Music";
    },
  },
  ALOUD_MUSIC_PAUSE: { label: () => "Pausing Music" },
  ALOUD_MUSIC_NEXT: { label: () => "Skipping to the next track" },
  ALOUD_MUSIC_NOW_PLAYING: { label: () => "Checking what's playing" },
  ALOUD_REMINDERS_ADD: {
    label: (a) => {
      const t = text(a, "text");
      return t ? `Adding a reminder: ${t}` : "Adding a reminder";
    },
  },
  ALOUD_NOTES_APPEND: {
    label: (a) => {
      const title = text(a, "title");
      return title ? `Adding a line to "${title}"` : "Appending to a note";
    },
    body: (a) => {
      const line = text(a, "text");
      return line ? (
        <ToolCard>
          <p className="m-0 whitespace-pre-wrap text-ui leading-relaxed text-muted-foreground">
            {line}
          </p>
        </ToolCard>
      ) : null;
    },
  },
  // The one with real blast radius. Same treatment GMAIL_SEND_EMAIL got after
  // the wrong-recipient bug: the gate shows the FULL handle, never abbreviated,
  // and the recipient is editable so a name that resolved to nothing — or to the
  // wrong person — is fixable before it sends.
  ALOUD_MESSAGES_SEND: {
    label: (a) => {
      const to = text(a, "to");
      return to ? `Messaging ${to}` : "Sending a message";
    },
    body: (a) => {
      const to = text(a, "to");
      const msg = text(a, "text");
      return (
        <ToolCard>
          {to ? (
            <p className="m-0 break-all text-ui text-muted-foreground">
              To: <span className="text-ink">{to}</span>
            </p>
          ) : (
            <p className="m-0 text-ui italic text-warning">
              No recipient — cancel and ask again.
            </p>
          )}
          {msg ? (
            <p className="m-0 whitespace-pre-wrap text-ui leading-relaxed text-muted-foreground">
              {msg}
            </p>
          ) : (
            <p className="m-0 text-ui italic text-warning">
              No message — cancel and ask again.
            </p>
          )}
        </ToolCard>
      );
    },
    edits: [
      {
        key: "to",
        label: "To",
        placeholder: "+1 555 123 4567 or email",
        get: (a) => text(a, "to"),
        set: (args, value) => ({ ...args, to: value }),
      },
    ],
  },
  ALOUD_FINDER_SEARCH: {
    // The grid renders the results; this is just the step label above it.
    label: (a) => {
      const q = text(a, "query");
      return q ? `Searching your Mac for "${q}"` : "Searching your Mac";
    },
  },
  ALOUD_FINDER_REVEAL: {
    label: (a) => {
      const path = text(a, "path");
      return path ? `Revealing ${path.split("/").pop() || path}` : "Revealing in Finder";
    },
  },
  ALOUD_SYSTEM_VOLUME: {
    label: (a) => {
      const level = a["level"];
      return typeof level === "number" ? `Setting volume to ${level}` : "Setting the volume";
    },
  },
  ALOUD_RUN_SHORTCUT: {
    label: (a) => {
      const name = text(a, "name");
      return name ? `Running "${name}"` : "Running a shortcut";
    },
  },
  ALOUD_OPEN_APP: {
    label: (a) => {
      // One call can open many, so the gate has to name them — "opening an
      // app" while fifteen launch is not something anyone can approve.
      const names = nested(a, "names");
      const list = Array.isArray(names) ? names.map(String) : [];
      if (list.length > 3) return `Opening ${list.length} apps`;
      if (list.length) return `Opening ${list.join(", ")}`;
      return text(a, "name") ? `Opening ${text(a, "name")}` : "Opening an app";
    },
  },
  ALOUD_COLLECT_FILES: {
    label: (a) => {
      const paths = nested(a, "paths");
      const count = Array.isArray(paths) ? paths.length : 0;
      const verb = String(a["move"]) === "true" ? "Moving" : "Copying";
      const where = text(a, "destination");
      const what = count ? `${count} file${count === 1 ? "" : "s"}` : "files";
      return where ? `${verb} ${what} to ${where}` : `${verb} ${what}`;
    },
  },
  ALOUD_ORGANIZE_FOLDER: {
    // The gate is approving a bulk move of real files, so the label names the
    // folder and the scheme — "tidy up" with no detail is not a gate.
    label: (a) => {
      const folder = text(a, "folder") || "Downloads";
      // `display_args` stringifies every non-string argument, so the rules map
      // arrives as JSON text — `nested` is the existing undo for that.
      const rules = nested(a, "rules");
      const names =
        rules && typeof rules === "object" ? Object.keys(rules as Record<string, unknown>) : [];
      return names.length
        ? `Sorting ${folder} into ${names.join(", ")}`
        : `Sorting your ${folder}`;
    },
  },
  ALOUD_UNDO_ORGANIZE: {
    label: (a) => `Putting ${text(a, "folder") || "Downloads"} back as it was`,
  },
  ALOUD_WINDOW_ARRANGE: {
    label: (a) => {
      const position = text(a, "position");
      const where: Record<string, string> = {
        full: "filling the screen",
        left: "to the left half",
        right: "to the right half",
        center: "to the centre",
      };
      return position
        ? `Moving the window ${where[position] ?? `to ${position}`}`
        : "Arranging the window";
    },
  },
  ALOUD_BROWSER_TAB: {
    label: (a) =>
      text(a, "action") === "current"
        ? "Checking the current page"
        : `Opening ${text(a, "url") || "a page"}`,
  },
  ALOUD_CLIPBOARD: {
    label: (a) =>
      text(a, "action") === "read" ? "Reading the clipboard" : "Copying to the clipboard",
  },
  ALOUD_QUIT_APP: {
    label: (a) => {
      const name = text(a, "name");
      return name ? `Quitting ${name}` : "Quitting an app";
    },
  },
  // Screen control. The click gate is the one confirmation a computer-use run
  // gets, so its label has to say what will be clicked — the model passes the
  // element's own text for exactly this, and a bare coordinate pair is a gate
  // nobody can judge.
  ALOUD_SCREEN_READ: { label: () => "Reading the screen" },
  ALOUD_SCREEN_CLICK: {
    label: (a) => {
      const what = text(a, "label");
      return what ? `Clicking "${what}"` : "Clicking on screen";
    },
  },
  ALOUD_SCREEN_TYPE: {
    label: (a) => {
      const typed = text(a, "text");
      return typed ? `Typing "${typed}"` : "Typing on screen";
    },
  },
  ALOUD_SCREEN_KEY: {
    label: (a) => {
      const keys = text(a, "keys");
      return keys ? `Pressing ${keys}` : "Pressing a key";
    },
  },
};

/** The gate's editable fields for a tool; empty for the yes/no majority. */
export function editsFor(name: string): Editable[] {
  return viewFor(name).edits ?? [];
}

/**
 * A nested argument, back as a value.
 *
 * `display_args` in agent.rs stringifies every argument that is not already a
 * string, so an array of objects reaches the panel as JSON text. Every other
 * view here reads flat string fields and never noticed; this one has to undo it.
 */
function nested(args: ToolArgs, field: string): unknown {
  const value = args[field];
  if (typeof value !== "string") return value;
  try {
    return JSON.parse(value);
  } catch {
    // Not JSON after all — a plain string field that happens to be nested-ish.
    return null;
  }
}

/** First string at `key` in an array of People API objects, or "". */
function firstOf(args: ToolArgs, field: string, key: string): string {
  const list = nested(args, field);
  if (!Array.isArray(list)) return "";
  for (const entry of list) {
    if (entry && typeof entry === "object") {
      const value = (entry as Record<string, unknown>)[key];
      if (typeof value === "string" && value) return value;
    }
  }
  return "";
}

function contactName(args: ToolArgs): string {
  const display = firstOf(args, "names", "displayName");
  if (display) return display;
  return [
    firstOf(args, "names", "givenName"),
    firstOf(args, "names", "familyName"),
  ]
    .filter(Boolean)
    .join(" ");
}

function contactEmail(args: ToolArgs): string {
  return firstOf(args, "emailAddresses", "value");
}

/** "SLACK_SEND_MESSAGE" → "Slack: send message"; the unregistered fallback. */
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

/** The question an editable gate opens with, or "" when it has none. */
export function askFor(name: string, args: ToolArgs): string {
  try {
    return viewFor(name).ask?.(args) ?? "";
  } catch {
    return "";
  }
}
