import type {
  ActionPreview,
  CardKind,
  Entry,
  Field,
  FinderResults,
  FoundFile,
  Role,
  Island,
} from "@/lib/bindings";
import type { Run } from "@/lib/agent-steps";
import type { ToolArgs } from "@/lib/tool-views";

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

/**
 * Past voice runs as history.sqlite hands them over: both kinds, a failure and
 * a long answer, newest first. Times are relative to the page load, so a
 * screenshot always reads "now, 4m, 1h…".
 */
export const VOICE_RUN_PREVIEWS: Entry[] = (() => {
  const now = Date.now() / 1000;
  const row = (
    id: number,
    ago: number,
    kind: Entry["kind"],
    said: string,
    outcome: string,
    appName: string | null,
    success = true,
  ): Entry => ({
    id,
    createdAt: now - ago,
    kind,
    rawTranscript: said,
    finalTranscript: outcome,
    success,
    appName,
    durationMs: 1800,
    utteranceId: id,
    audioPath: null,
  });
  return [
    row(7, 20, "command", "What is the capital of Australia", "Canberra. It was picked in 1908 as a compromise between Sydney and Melbourne, and Parliament moved there in 1927.", "Safari"),
    row(6, 4 * 60, "dictation", "um let's meet on thursday at three", "Let's meet on Thursday at three.", "Messages"),
    row(5, 22 * 60, "command", "Email David the project update", "Gmail is not connected. Connect it in Settings and ask again.", "Mail", false),
    row(4, 3600, "command", "Pause the music", "Paused Spotify.", null),
    row(3, 5 * 3600, "dictation", "Sounds good, see you then", "Sounds good, see you then.", "Slack"),
    row(2, 26 * 3600, "command", "Create a note called groceries with milk, eggs and flour", "Created the note Groceries.", "Notes"),
    row(1, 50 * 3600, "dictation", "Remind me to call the dentist tomorrow morning", "Remind me to call the dentist tomorrow morning.", null),
  ];
})();

/** How fast `partial: "stream"` advances, in ms between revisions. */
export const PARTIAL_MS = 320;

/** A field as Rust sends it, with only what differs from empty spelled out. */
function f(
  role: Role,
  path: string,
  label: string,
  value: string,
  rest: Partial<Field> = {},
): Field {
  return {
    role,
    path,
    label,
    value,
    items: [],
    count: 0,
    name: null,
    unresolved: false,
    before: null,
    origin: null,
    editable: false,
    cells: [],
    ...rest,
  };
}

type Card = Pick<
  ActionPreview,
  "kind" | "verb" | "title" | "fields" | "set" | "total"
> &
  Partial<ActionPreview>;

/** A run stopped at one write's gate. */
function gate(
  heard: string,
  name: string,
  app: string,
  args: ToolArgs,
  card: Card,
): Run {
  return {
    heard,
    steps: [
      {
        name,
        app,
        args,
        requiresConfirmation: true,
        preview: {
          tier: "confirm",
          more: [],
          risks: [],
          pickedBy: null,
          recoverable: null,
          safer: null,
          autoMs: null,
          revision: 0,
          scope: null,
          ...card,
        },
      },
    ],
  };
}

/** Three a read named, then 35 more the same search returned. */
const DELETE_IDS = [
  "18c2a",
  "18c1b",
  "18c0c",
  ...Array.from({ length: 35 }, (_, i) => `18b${String(i).padStart(2, "0")}`),
];

/**
 * One gate per card kind, so every layout can be checked by eye from the
 * tuning page. Each preview is what `aloud_agent::preview::render` and
 * `enrich` made of these arguments against the live schemas in
 * `crates/agent/src/preview/fixtures` (HubSpot's is a trimmed stand-in), with
 * the ids an earlier read in the run would have named — so a mismatch with the
 * design is a real one, not a fixture typo.
 */
export const CARD_PREVIEWS: Record<CardKind, Run> = {
  message: gate(
    "send hi to the launch channel on slack",
    "SLACK_CHAT_POST_MESSAGE",
    "Slack",
    { channel: "C07LQ2M9K", markdown_text: "hi" },
    {
      kind: "message",
      verb: "Send",
      title: "Send message to launch",
      fields: [
        f("destination", "/channel", "Channel", "C07LQ2M9K", {
          name: "launch",
        }),
        f("text", "/markdown_text", "Message", "hi", {
          origin: "yours",
          editable: true,
        }),
      ],
      set: 2,
      total: 12,
      // The user's own words to a channel a read named: it counts down.
      autoMs: 3000,
    },
  ),
  email: gate(
    "email david the project update and ask to sync thursday",
    "GMAIL_SEND_EMAIL",
    "Gmail",
    {
      body: "Hi David,\n\nThe designs look great. Let's sync Thursday to walk through the rollout.\n\nThanks",
      recipient_email: "david@company.com",
      subject: "Project update and Thursday sync",
    },
    {
      kind: "email",
      verb: "Send",
      title: "Send email to david@company.com",
      fields: [
        f("person", "/recipient_email", "To", "david@company.com"),
        f("heading", "/subject", "Subject", "Project update and Thursday sync", {
          editable: true,
        }),
        f(
          "text",
          "/body",
          "Body",
          "Hi David,\n\nThe designs look great. Let's sync Thursday to walk through the rollout.\n\nThanks",
          { origin: "aloud", editable: true },
        ),
      ],
      set: 3,
      total: 10,
    },
  ),
  event: gate(
    "lunch with priya friday at 1",
    "GOOGLECALENDAR_CREATE_EVENT",
    "Google Calendar",
    {
      attendees: ["priya@acme.co"],
      event_duration_hour: 1,
      start_datetime: "2026-10-09T13:00:00",
      summary: "Lunch with Priya",
      timezone: "Asia/Kolkata",
    },
    {
      kind: "event",
      verb: "Create",
      title: "Create event with priya@acme.co",
      fields: [
        f("heading", "/summary", "Title", "Lunch with Priya", {
          editable: true,
        }),
        f("when", "/start_datetime", "Starts", "Fri, Oct 9, 2026, 1:00 PM", {
          items: ["Asia/Kolkata · 1 hr"],
        }),
        f("person", "/attendees", "Guests", "priya@acme.co", {
          items: ["priya@acme.co"],
          count: 1,
        }),
        f("flag", "/create_meeting_room", "Google Meet link", "Yes"),
      ],
      set: 5,
      total: 25,
      risks: ["Guests get an invite email.", "Adds a Google Meet link."],
    },
  ),
  task: gate(
    "remind me to call ana at 5",
    "GOOGLETASKS_INSERT_TASK",
    "Google Tasks",
    { due: "2026-10-03T17:00:00", tasklist_id: "@default", title: "Call Ana" },
    {
      kind: "task",
      verb: "Add",
      title: "Add task",
      fields: [
        f("text", "/title", "Task", "Call Ana", {
          origin: "yours",
          editable: true,
        }),
        f("when", "/due", "Due", "Sat, Oct 3, 2026, 5:00 PM"),
        f("id", "/tasklist_id", "List", "@default", { unresolved: true }),
      ],
      set: 3,
      total: 12,
    },
  ),
  note: gate(
    "add a risks page to the launch prd in notion",
    "NOTION_CREATE_NOTION_PAGE",
    "Notion",
    {
      markdown:
        "## Risks\n- App Store review could slip the date\n- Payments migration lands the same week",
      parent_id: "59833787-2cf9-4fdf-8782-e53db20768a5",
      title: "Risks",
    },
    {
      kind: "note",
      verb: "Create",
      title: "Create page",
      fields: [
        f("heading", "/title", "Title", "Risks", { editable: true }),
        f(
          "id",
          "/parent_id",
          "Inside",
          "59833787-2cf9-4fdf-8782-e53db20768a5",
          { name: "Launch PRD" },
        ),
        f(
          "text",
          "/markdown",
          "Content",
          "## Risks\n- App Store review could slip the date\n- Payments migration lands the same week",
          { origin: "aloud", editable: true },
        ),
      ],
      set: 3,
      total: 5,
    },
  ),
  issue: gate(
    "file a linear bug, login loops on ios, assign it to priya, high priority",
    "LINEAR_CREATE_LINEAR_ISSUE",
    "Linear",
    {
      assignee_id: "b86e2b0f-6b1a-4db8-9731-41ef97f5ba8e",
      description:
        "After entering the code, sign-in on iOS returns to the login screen.",
      priority: 2,
      team_id: "9a7f1e3c-4d5b-6a7f-8e9d-0a1b2c3d4e5f",
      title: "Login loops on iOS",
    },
    {
      kind: "issue",
      verb: "Create",
      title: "Create issue",
      fields: [
        f("heading", "/title", "Title", "Login loops on iOS", {
          editable: true,
        }),
        f("id", "/team_id", "Team", "9a7f1e3c-4d5b-6a7f-8e9d-0a1b2c3d4e5f", {
          name: "Mobile",
        }),
        f(
          "text",
          "/description",
          "Description",
          "After entering the code, sign-in on iOS returns to the login screen.",
          { origin: "aloud", editable: true },
        ),
        f(
          "id",
          "/assignee_id",
          "Assignee",
          "b86e2b0f-6b1a-4db8-9731-41ef97f5ba8e",
          { name: "Priya Shah" },
        ),
        f("plain", "/priority", "Priority", "High"),
      ],
      set: 5,
      total: 12,
      risks: ["The assignee gets notified."],
    },
  ),
  row: gate(
    "log 42 dollars lunch to expenses",
    "GOOGLESHEETS_SPREADSHEETS_VALUES_APPEND",
    "Google Sheets",
    {
      insertDataOption: "INSERT_ROWS",
      range: "Expenses!A:D",
      spreadsheetId: "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
      valueInputOption: "USER_ENTERED",
      values: [["3 Oct", "Lunch", 42, "Food"]],
    },
    {
      kind: "row",
      verb: "Add",
      title: "Add row",
      fields: [
        f(
          "id",
          "/spreadsheetId",
          "Sheet",
          "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms",
          { name: "Expenses 2026" },
        ),
        f("plain", "/range", "Range", "Expenses!A:D"),
        f("list", "/values", "Rows", "3 Oct, Lunch, 42, Food", {
          items: ["3 Oct, Lunch, 42, Food"],
          count: 1,
          cells: [
            ["A", "B", "C", "D"],
            ["3 Oct", "Lunch", "42", "Food"],
          ],
        }),
        f("pill", "/insertDataOption", "Mode", "INSERT_ROWS"),
      ],
      more: [
        f("pill", "/valueInputOption", "Value input option", "USER_ENTERED"),
      ],
      set: 5,
      total: 9,
    },
  ),
  file: gate(
    "move the tax pdf from downloads to my taxes folder",
    "ALOUD_COLLECT_FILES",
    "macOS",
    {
      destination: "~/Documents/Taxes",
      move: true,
      paths: ["~/Downloads/tax-2025.pdf"],
    },
    {
      kind: "file",
      verb: "Collect",
      title: "Collect files",
      fields: [
        f("list", "/paths", "Files", "~/Downloads/tax-2025.pdf", {
          items: ["~/Downloads/tax-2025.pdf"],
          count: 1,
        }),
        f("path", "/destination", "Into", "~/Documents/Taxes"),
        f("flag", "/move", "Move, not copy", "Yes"),
      ],
      set: 3,
      total: 3,
      risks: ["Moves the originals out of where they are now."],
    },
  ),
  share: gate(
    "share the q4 deck with dana at acme so she can edit",
    "GOOGLEDRIVE_CREATE_PERMISSION",
    "Google Drive",
    {
      email_address: "dana@acme.co",
      file_id: "1a2b3c4d5e",
      role: "writer",
      send_notification_email: true,
      type: "user",
    },
    {
      kind: "share",
      verb: "Share",
      title: "Share file with dana@acme.co",
      fields: [
        f("id", "/file_id", "File", "1a2b3c4d5e", { name: "Q4 Deck" }),
        f("person", "/email_address", "With", "dana@acme.co"),
        f("pill", "/type", "Who", "user"),
        f("pill", "/role", "Access", "writer"),
        f("flag", "/send_notification_email", "Email them", "Yes"),
      ],
      set: 5,
      total: 13,
    },
  ),
  update: gate(
    "move lunch with priya to 1pm",
    "GOOGLECALENDAR_UPDATE_EVENT",
    "Google Calendar",
    {
      event_id: "ev1",
      start_datetime: "2026-10-09T13:00:00",
      summary: "Lunch with Priya",
    },
    {
      kind: "update",
      verb: "Update",
      title: "Update event",
      fields: [
        f("id", "/event_id", "Event", "ev1", { name: "Lunch" }),
        f("heading", "/summary", "Title", "Lunch with Priya", {
          before: "Lunch",
          editable: true,
        }),
        f("when", "/start_datetime", "Starts", "Fri, Oct 9, 2026, 1:00 PM", {
          before: "Fri, Oct 9, 2026, 12:00 PM",
        }),
        f("flag", "/create_meeting_room", "Google Meet link", "Yes"),
      ],
      set: 3,
      total: 25,
      risks: ["No guests are listed, so any on the event now are removed."],
    },
  ),
  delete: gate(
    "delete all the newsletters older than a month",
    "GMAIL_BATCH_DELETE_MESSAGES",
    "Gmail",
    { messageIds: DELETE_IDS },
    {
      kind: "delete",
      verb: "Delete",
      title: "Delete 38 emails",
      fields: [
        f(
          "list",
          "/messageIds",
          "Emails",
          "Morning Brew · Tuesday's edition, Figma · Config 2026 replays are up, Substack · Weekly digest and 35 more",
          {
            items: [
              "Morning Brew · Tuesday's edition",
              "Figma · Config 2026 replays are up",
              "Substack · Weekly digest",
            ],
            count: 38,
          },
        ),
      ],
      set: 1,
      total: 2,
      tier: "strict",
      risks: ["These skip the Trash. Gmail cannot get them back."],
      pickedBy: "category:promotions older_than:30d",
      safer: { label: "Move to Trash", tool: "GMAIL_MOVE_TO_TRASH" },
    },
  ),
  money: gate(
    "charge acme 42 dollars for the extra seats",
    "STRIPE_CREATE_PAYMENT_INTENT",
    "Stripe",
    {
      amount: 4200,
      confirm: true,
      currency: "usd",
      customer: "cus_Q3f2xY",
      payment_method: "pm_1NvisaX",
      receipt_email: "billing@acme.co",
    },
    {
      kind: "money",
      verb: "Charge",
      title: "Charge Acme Inc",
      fields: [
        f("money", "/amount", "Amount", "42.00 USD"),
        f("id", "/customer", "Customer", "cus_Q3f2xY", { name: "Acme Inc" }),
        f("id", "/payment_method", "Card", "pm_1NvisaX", { unresolved: true }),
        f("flag", "/confirm", "Charge now", "Yes"),
        f("person", "/receipt_email", "Receipt to", "billing@acme.co"),
      ],
      set: 6,
      total: 18,
      tier: "strict",
      risks: ["Charges the card now. A refund is a separate action."],
    },
  ),
  fallback: gate(
    "add dana lee from acme to hubspot, she runs ops",
    "HUBSPOT_CREATE_CONTACT",
    "HubSpot",
    {
      company: "Acme",
      email: "dana@acme.co",
      firstname: "Dana",
      hubspot_owner_id: "81234567",
      jobtitle: "Head of Ops",
      lastname: "Lee",
    },
    {
      kind: "fallback",
      verb: "Create",
      title: "Create contact dana@acme.co",
      fields: [
        f("person", "/email", "Email", "dana@acme.co"),
        f("id", "/hubspot_owner_id", "Owner", "81234567", { unresolved: true }),
        f("plain", "/company", "Company", "Acme"),
        f("plain", "/firstname", "First name", "Dana"),
        f("plain", "/jobtitle", "Job title", "Head of Ops"),
        f("plain", "/lastname", "Last name", "Lee"),
      ],
      set: 6,
      total: 8,
    },
  ),
};

/**
 * The contact card `hand.json` draws once both lookups came back empty: a
 * name and an empty, editable Email — the one field the agent could not fill.
 */
export const CONTACT_PREVIEW: ActionPreview = {
  ...(gate("", "GOOGLECONTACTS_CREATE_CONTACT", "Google Contacts", {}, {
    kind: "fallback",
    verb: "Save",
    title: "Save contact",
    fields: [
      f("plain", "/names/0/displayName", "Name", "Genghis Khan", { editable: true }),
      f("plain", "/names/0/givenName", "First name", "Genghis Khan", { editable: true }),
      f("person", "/emailAddresses/0/value", "Email", "", { editable: true }),
    ],
    set: 3,
    total: 28,
  }).steps[0].preview as ActionPreview),
  more: [f("plain", "/personFields", "Person Fields", "names,emailAddresses")],
};

/** A canned agent run for the tuning page: one write done, the next at its gate. */
export const AGENT_PREVIEW: Run = {
  heard: "Say hi in launch, then email David the project update",
  steps: [
    {
      ...CARD_PREVIEWS.message.steps[0],
      confirmed: true,
      result: { success: true, message: "Sent", show: null, note: null, tidied: null },
    },
    CARD_PREVIEWS.email.steps[0],
  ],
};

/** `run` with every step wearing `icon`, as the gate would hand it over. */
export function withIcon(run: Run, icon: string | undefined): Run {
  return icon ? { ...run, steps: run.steps.map((s) => ({ ...s, icon })) } : run;
}

/** A found file as `finder_search` reports one: label, size and date formatted in Rust. */
function found(
  name: string,
  size: string,
  modified: string,
  label = name,
  thumbnail: string | null = null,
): FoundFile {
  return { path: `/Users/y/Documents/Taxes/${name}`, name, label, size, modified, thumbnail };
}

/** A flat swatch standing in for a Quick Look thumbnail, which needs a Mac. */
const swatch = (hue: number) =>
  `data:image/svg+xml,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="hsl(${hue} 45% 55%)"/></svg>`,
  )}`;

const TAX_RETURNS: FinderResults = {
  query: "2025 Tax Returns",
  heading: "2025 Tax Returns — 10 items",
  count: 10,
  files: [
    found("2025 California Resident Income Tax Return.pdf", "4.9 KB", "8:02 PM", "2025 California…Tax Return.pdf"),
    found("2025 Federal Return 1040.pdf", "212 KB", "8:02 PM"),
    found("2025 Schedule C — Consulting.pdf", "88 KB", "Yesterday"),
    found("W-2 Acme Inc 2025.pdf", "61 KB", "Feb 12"),
    found("1099-NEC Contoso 2025.pdf", "44 KB", "Feb 3"),
    found("Charitable receipts 2025.xlsx", "19 KB", "Jan 28"),
  ],
};

/**
 * A finished Finder search, as the list documents get and as the image grid —
 * the results riding on the step, since the tuner has no Mac to search.
 */
export const FINDER_PREVIEWS: Record<"list" | "grid", Run> = {
  list: {
    heard: "Can you list last year's tax returns?",
    steps: [
      {
        name: "ALOUD_FINDER_SEARCH",
        app: "macOS",
        args: { query: "2025 Tax Returns", kind: "document" },
        requiresConfirmation: false,
        result: { success: true, message: "done", show: null, note: null, tidied: null },
        found: TAX_RETURNS,
      },
    ],
    finished: "Found 10 of your 2025 tax returns.",
  },
  grid: {
    heard: "Find my screenshots from the offsite",
    steps: [
      {
        name: "ALOUD_FINDER_SEARCH",
        app: "macOS",
        args: { query: "offsite", kind: "image" },
        requiresConfirmation: false,
        result: { success: true, message: "done", show: null, note: null, tidied: null },
        found: {
          query: "offsite",
          heading: "offsite — 6 items",
          count: 6,
          files: [20, 60, 120, 200, 260, 320].map((hue, i) =>
            found(`Offsite ${i + 1}.png`, "1.2 MB", "Oct 2", undefined, swatch(hue)),
          ),
        },
      },
    ],
    finished: "Found 6 screenshots from the offsite.",
  },
};

/** An alert island, as `agent.rs`'s `Toast::island` builds one. */
const alert = (id: string, title: string, app: string, actions: Island["actions"]): Island => ({
  id,
  kind: "alert",
  priority: "normal",
  icon: null,
  title,
  detail: app,
  actions,
  expires: 30_000,
  dismissible: false,
});

/** The chips a run of writes ends on, one per action a toast can offer. */
export const TOAST_PREVIEWS: Record<"show" | "undo" | "plain", Island> = {
  show: alert("toast:show", "Reminder: Send Maya the deck", "Reminders", [
    {
      label: "Show",
      action: "result.show",
      args: { app: "Reminders", target: "x-apple-reminder://r1" },
    },
  ]),
  undo: alert("toast:undo", "Sorted 24 files in Downloads", "macOS", [
    { label: "Undo", action: "files.undoTidy", args: { folder: "Downloads" } },
  ]),
  plain: alert("toast:plain", "Sent the email to David", "Gmail", []),
};

/**
 * Live islands for the closed notch: the agents one Rust builds today, and
 * the shapes later features will push, so the pill's layout with two and
 * three activities can be judged before they exist. In the order the crate
 * ranks them (high, normal, low): the preview has no Rust to rank them.
 */
export function livePreviews(n: number): Island[] {
  // One is today's: the agents. More add the shapes features will bring.
  return n === 1 ? LIVE_PREVIEWS.filter((i) => i.id === "agents") : LIVE_PREVIEWS.slice(0, n);
}

const LIVE_PREVIEWS: Island[] = [
  {
    id: "timer",
    kind: "live",
    priority: "high",
    icon: "timer",
    title: "Focus session",
    detail: "18:42",
    actions: [],
    expires: null,
    dismissible: false,
  },
  {
    id: "agents",
    kind: "live",
    priority: "normal",
    icon: "agents",
    title: "Claude Code · aloud-tauri",
    detail: "2 running",
    actions: [{ label: "Open", action: "agents.open", args: {} }],
    expires: null,
    dismissible: false,
  },
  {
    id: "download",
    kind: "live",
    priority: "low",
    icon: "download",
    title: "report-q3.pdf",
    detail: "74%",
    actions: [],
    expires: null,
    dismissible: false,
  },
];

/**
 * The takeovers, one per size and shape, as Rust's gate would hand them to the
 * webview (tier resolved, dwell set). Fixtures for the tuner and the browser
 * preview: no Rust here, so nothing ranks them or holds one back.
 */
function sysWhisper(
  id: string,
  icon: string,
  title: string,
  short: string | null,
  source: Island["source"],
): Island {
  return {
    id,
    kind: "alert",
    priority: "normal",
    icon,
    title,
    detail: null,
    actions: [],
    expires: null,
    dismissible: false,
    tier: "whisper",
    short,
    dwellMs: 1800,
    source,
  };
}

/**
 * A stand-in for an app's real icon, which Rust reads off disk
 * (`aloud_selection::app_icon`) and a browser cannot: a rounded tile in the
 * app's colour with its initial, as a `data:` URI like the real thing.
 */
function appTile(color: string, letter: string): string {
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" rx="15" fill="${color}"/>` +
    `<text x="32" y="44" font-family="-apple-system,sans-serif" font-size="34" font-weight="600" text-anchor="middle" fill="#fff">${letter}</text></svg>`;
  return `data:image/svg+xml;base64,${btoa(svg)}`;
}

/**
 * What the nudge builders in Rust send (`aloud_notify::island`,
 * `aloud_agenda::soon_island`): sender or title in `title`, the app in
 * `detail`, the line in `body`, and Open or Join as the only buttons, since
 * Aloud cannot answer a call or reply for you.
 */
function nudge(
  id: string,
  icon: string,
  title: string,
  app: string,
  body: string,
  rest: Partial<Island> = {},
): Island {
  return {
    id,
    kind: "alert",
    priority: "normal",
    icon,
    title,
    detail: app,
    body,
    short: title,
    actions: [{ label: "Open", action: "nudge.open", args: { bundle: id.split(":")[1] ?? "" } }],
    expires: null,
    dismissible: false,
    tier: "peek",
    dwellMs: 4000,
    source: "banners",
    ...rest,
  };
}

export const TAKEOVER_PREVIEWS = {
  /** A key press: a bar and its number, updated in place. */
  volume: {
    id: "hud:volume",
    kind: "alert",
    priority: "normal",
    icon: "volume",
    title: "Volume",
    detail: null,
    actions: [],
    expires: null,
    dismissible: false,
    tier: "whisper",
    level: 62,
    dwellMs: 1800,
    source: "hud",
  },
  /** A write that landed; Undo is one hover away. */
  sent: {
    id: "toast:sent",
    kind: "alert",
    priority: "normal",
    icon: "done",
    title: "Sent the email to David",
    detail: "Gmail",
    actions: [{ label: "Undo", action: "files.undoTidy", args: { folder: "Downloads" } }],
    expires: null,
    dismissible: false,
    tier: "whisper",
    short: "Sent",
    dwellMs: 1800,
  },
  /** Battery at 20%: a red number, the Power pane on hover. */
  batteryLow: {
    id: "mac:battery-low",
    kind: "alert",
    priority: "normal",
    icon: "battery",
    title: "Battery low",
    detail: "20%",
    actions: [{ label: "Show", action: "mac.power", args: {} }],
    expires: null,
    dismissible: false,
    tier: "whisper",
    short: "20%",
    dwellMs: 1800,
    source: "power",
  },
  /** A screenshot: a peek, one extra row. */
  shot: {
    id: "shot:/Users/me/Desktop/Screenshot.png",
    kind: "alert",
    priority: "normal",
    icon: "image",
    title: "Screenshot saved",
    detail: "Screenshots",
    actions: [
      { label: "Copy", action: "shelf.copyShot", args: { path: "/Users/me/Desktop/Screenshot.png" } },
      { label: "Show", action: "shelf.reveal", args: { path: "/Users/me/Desktop/Screenshot.png" } },
    ],
    expires: null,
    dismissible: false,
    tier: "peek",
    dwellMs: 4000,
    source: "banners",
  },
  /** A focus phase ending: high, so it cuts in on a whisper. */
  timer: {
    id: "timer:done",
    kind: "alert",
    priority: "high",
    icon: "timer",
    title: "Focus done",
    detail: "Focus",
    actions: [{ label: "Start", action: "focus.resume", args: {} }],
    expires: null,
    dismissible: false,
    tier: "peek",
    dwellMs: 4000,
  },
  /** The same peek, shrunk to a capsule because the user is typing. */
  timerTyping: {
    id: "timer:done",
    kind: "alert",
    priority: "high",
    icon: "timer",
    title: "Focus done",
    detail: "Focus",
    actions: [{ label: "Start", action: "focus.resume", args: {} }],
    expires: null,
    dismissible: false,
    tier: "peek",
    shrunk: true,
    short: "Focus done",
    dwellMs: 4000,
  },
  /** Battery under 10%: a card, sticky, with the way to act. */
  batteryCritical: {
    id: "mac:battery",
    kind: "alert",
    priority: "urgent",
    icon: "battery",
    title: "Battery low",
    detail: "9% — plug in now",
    actions: [{ label: "Power", action: "mac.power", args: {} }],
    expires: null,
    dismissible: true,
    tier: "card",
    short: "9%",
    source: "power",
  },
  /** A critical action that failed: a card with a way to retry. */
  failed: {
    id: "toast:failed",
    kind: "alert",
    priority: "high",
    icon: "alert",
    title: "Couldn't send to #design",
    detail: "Slack rejected the token",
    actions: [{ label: "Retry", action: "agents.open", args: {} }],
    expires: null,
    dismissible: false,
    tier: "card",
  },
  // The system events, one per colour the design gives them.
  caps: sysWhisper("hud:caps", "caps", "Caps Lock", "Caps Lock", "hud"),
  micMuted: sysWhisper("hud:mic", "mic-off", "Microphone", "Mic muted", "hud"),
  brightness: { ...sysWhisper("hud:brightness", "brightness", "Brightness", null, "hud"), level: 70 },
  airpods: sysWhisper("bt:70:ae:2a", "headphones", "AirPods Pro", "88%", "connectivity"),
  charger: sysWhisper("power:charger", "bolt", "Charging", "46%", "power"),
  lowPower: sysWhisper("power:lowpower", "battery", "Low Power Mode on", "Low Power", "power"),
  wifi: sysWhisper("net:wifi", "wifi", "Wi-Fi connected", "Wi-Fi on", "connectivity"),
  drive: sysWhisper("drive:/Volumes/Backup", "drive", "Backup connected", "Connected", "connectivity"),
  // Nudges: one per kind, the events design's msg, mail, slack, cal, rem, call,
  // airdrop and generic.
  nudgeMsg: nudge(
    "nudge:com.apple.MobileSMS:41",
    appTile("#34c85a", "M"),
    "Maya Chen",
    "Messages",
    "Are we still on for 3? I'm bringing the mockups.",
  ),
  /** The same message while the user types: a side capsule, opened by pointing. */
  nudgeMsgTyping: nudge(
    "nudge:com.apple.MobileSMS:41",
    appTile("#34c85a", "M"),
    "Maya Chen",
    "Messages",
    "Are we still on for 3? I'm bringing the mockups.",
    { shrunk: true },
  ),
  nudgeMail: nudge(
    "nudge:com.apple.mail:42",
    appTile("#0a84ff", "M"),
    "Linear",
    "Mail",
    "Weekly digest · 12 issues closed this week",
  ),
  nudgeSlack: nudge(
    "nudge:com.tinyspeck.slackmacgap:43",
    appTile("#4a154b", "S"),
    "Priya",
    "Slack · #design",
    "Mockups for the notch are in the doc, can you look?",
  ),
  nudgeCal: nudge("cal:soon:standup", appTile("#ff4539", "31"), "Standup in 10 minutes", "Calendar", "10:30 AM · Zoom", {
    priority: "high",
    actions: [{ label: "Join", action: "calendar.join", args: { url: "https://zoom.us/j/1" } }],
  }),
  nudgeRem: nudge("rem:due:dentist", appTile("#fea00a", "R"), "Call the dentist", "Reminders", "Due 3:00 PM · Personal", {
    actions: [{ label: "Open", action: "nudge.open", args: { bundle: "com.apple.reminders" } }],
  }),
  nudgeAirdrop: nudge(
    "nudge:com.apple.sharingd:44",
    appTile("#0a84ff", "A"),
    "Maya Chen",
    "AirDrop",
    "IMG_2041.heic",
    { priority: "high" },
  ),
  nudgeGeneric: nudge(
    "nudge:com.culturedcode.ThingsMac:45",
    appTile("#3478f6", "T"),
    "Review budget",
    "Things",
    "Due today at 5:00 PM · Finance",
  ),
  /** A ringing call: a card, urgent, with the one thing Aloud can honestly do. */
  nudgeCall: nudge("nudge:com.apple.FaceTime:46", appTile("#34c85a", "F"), "Maya Chen", "FaceTime", "FaceTime Video · iPhone", {
    priority: "urgent",
    tier: "card",
    dwellMs: null,
  }),
} satisfies Record<string, Island>;

export type TakeoverPreview = keyof typeof TAKEOVER_PREVIEWS;
