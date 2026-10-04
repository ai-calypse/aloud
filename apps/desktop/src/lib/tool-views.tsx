/**
 * What the agent panel draws for each tool. A registry rather than a switch:
 * any of Composio's 800+ toolkits can be connected, so a service's custom UI
 * is one entry. Anything unregistered falls back to a humanised label. What a
 * write's approval card shows is Rust's (`aloud_agent::preview`), not this.
 */

/** A tool call's arguments. Unknown shape — every read must be defensive. */
export type ToolArgs = Record<string, unknown>;

export type ToolView = {
  /** Present tense, what is happening: "Drafting an email to Ana". */
  label: (args: ToolArgs) => string;
  /**
   * Fields the approval gate lets the user fill in before confirming, in place
   * of its card. Only worth declaring where the model routinely cannot know
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
  /**
   * The approval gate's title, where "<label> — this writes to <app>" would
   * misstate what a yes covers.
   */
  gate?: (args: ToolArgs) => string;
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

/** "someone@example.com" → "someone", for a label that fits one line. */
function who(address: string): string {
  return address.split("@")[0] || address;
}

/** A directory lookup, named for who it is looking for. */
function lookingFor(query: string): string {
  return query ? `Looking for ${query}` : "Looking through your contacts";
}

const VIEWS: Record<string, ToolView> = {
  GMAIL_SEND_EMAIL: {
    label: (a) => {
      const to = text(a, "recipient_email");
      return to ? `Sending an email to ${who(to)}` : "Sending an email";
    },
  },
  GMAIL_CREATE_EMAIL_DRAFT: {
    label: (a) => {
      const to = text(a, "recipient_email");
      return to ? `Drafting an email to ${who(to)}` : "Drafting an email";
    },
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
  },
  GOOGLECALENDAR_UPDATE_EVENT: {
    label: (a) => {
      const title = text(a, "summary");
      return title ? `Updating "${title}"` : "Updating a calendar event";
    },
  },
  GOOGLECALENDAR_FIND_EVENT: { label: () => "Looking through your calendar" },
  GOOGLECALENDAR_FIND_FREE_SLOTS: { label: () => "Finding a free slot" },
  GOOGLECALENDAR_DELETE_EVENT: {
    label: () => "Deleting a calendar event",
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
  ALOUD_MUSIC: { label: (a) => mediaLabel("Music", a) },
  ALOUD_SPOTIFY: { label: (a) => mediaLabel("Spotify", a) },
  ALOUD_REMINDERS_ADD: {
    label: (a) => {
      const t = text(a, "text");
      const due = text(a, "due");
      if (!t) return "Adding a reminder";
      return due ? `Adding a reminder: ${t} (${due.replace("T", " ")})` : `Adding a reminder: ${t}`;
    },
  },
  ALOUD_NOTES_CREATE: {
    label: (a) => {
      const title = text(a, "title");
      return title ? `Creating the note "${title}"` : "Creating a note";
    },
  },
  ALOUD_NOTES_APPEND: {
    label: (a) => {
      const title = text(a, "title");
      return title ? `Adding to "${title}"` : "Adding to the open note";
    },
  },
  // The one with real blast radius: the recipient is editable so a name that
  // resolved to nothing — or to the wrong person — is fixable before it sends.
  ALOUD_MESSAGES_SEND: {
    label: (a) => {
      const to = text(a, "to");
      return to ? `Messaging ${to}` : "Sending a message";
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
  ALOUD_DISCORD_MESSAGE: {
    label: (a) => {
      const to = text(a, "to");
      return to ? `Messaging ${to} on Discord` : "Sending a Discord message";
    },
    edits: [
      {
        key: "to",
        label: "To",
        placeholder: "Name as it appears in Discord",
        get: (a) => text(a, "to"),
        set: (args, value) => ({ ...args, to: value }),
      },
    ],
  },
  ALOUD_PHOTO_BOOTH_TAKE_PHOTO: { label: () => "Taking a photo in Photo Booth" },
  ALOUD_PRESS_KEYS: {
    label: (a) => {
      const keys = text(a, "keys");
      return keys ? `Pressing ${keys}` : "Pressing a shortcut";
    },
  },
  ALOUD_MENU: {
    label: (a) => {
      if (a.list === true) return "Reading the menus";
      const item = text(a, "item") || (Array.isArray(a.path) ? a.path.join(" › ") : "");
      return item ? `Choosing ${item}` : "Choosing a menu command";
    },
  },
  ALOUD_SCREEN_READ: {
    label: (a) => {
      const app = text(a, "app");
      return app ? `Reading ${app}'s window` : "Reading the screen";
    },
  },
  // One yes covers the run: a screen task is read, click, read, type, and a
  // prompt per click would have the user answering faster than it works.
  ALOUD_SCREEN_CLICK: {
    label: (a) => (text(a, "app") ? `Clicking in ${text(a, "app")}` : "Clicking"),
    gate: (a) =>
      `Let Aloud click and type in ${text(a, "app") || "this app"} until this command is done?`,
  },
  ALOUD_SCREEN_TYPE: {
    label: (a) => {
      const typed = text(a, "text");
      const app = text(a, "app");
      return typed ? `Typing "${typed}"${app ? ` in ${app}` : ""}` : "Typing";
    },
    gate: (a) =>
      `Let Aloud click and type in ${text(a, "app") || "this app"} until this command is done?`,
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
  ALOUD_WRITE_FILE: {
    label: (a) => {
      const path = text(a, "path");
      const name = path.split("/").pop() || path;
      return name ? `Saving ${name}` : "Saving a file";
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
};

/** "Playing lofi in Spotify", "Pausing Music": one label for every media action. */
function mediaLabel(app: string, a: Record<string, unknown>): string {
  const what = text(a, "query") || text(a, "uri");
  switch (text(a, "action")) {
    case "play":
      return what ? `Playing ${what} in ${app}` : `Resuming ${app}`;
    case "pause":
      return `Pausing ${app}`;
    case "next":
      return `Skipping to the next track in ${app}`;
    case "previous":
      return `Going back a track in ${app}`;
    default:
      return `Checking what's playing in ${app}`;
  }
}

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

/** The approval gate's own title, or "" to use the label. */
export function gateFor(name: string, args: ToolArgs): string {
  try {
    return viewFor(name).gate?.(args) ?? "";
  } catch {
    return "";
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
