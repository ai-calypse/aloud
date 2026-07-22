import { useEffect, useState } from "react";
import { commands, events, type MeetingPhase, type Note } from "../../lib/bindings";
import { or } from "../../lib/result";
import { Button } from "@/components/ui/button";
import { GridRow, Group, PaneTitle, Surface } from "@/components/ui/rows";
import { NoteReader } from "../notes/NotesView";
import { formatDate, formatElapsed } from "@/lib/format";

/**
 * Meeting capture: the running session, and the notes past ones left behind.
 * Every note in the notes directory is a meeting note (`write_note` has one
 * caller), so past meetings are just `recentNotes`.
 */
export function MeetingsView() {
  // `at` is when the payload arrived, so the timer can advance between events.
  const [live, setLive] = useState<{ phase: MeetingPhase; at: number } | null>(
    null,
  );
  const [past, setPast] = useState<Note[]>([]);
  const [open, setOpen] = useState<Note | null>(null);
  // Read from the clock rather than counted up, because this window App Naps
  // while unfocused: a timer that stops firing would resume minutes behind.
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const arrive = (phase: MeetingPhase | null) =>
      setLive(phase && phase.phase !== "idle" ? { phase, at: Date.now() } : null);

    // A poll must never *clear* a saving state: `meeting_status` answers null
    // for the whole summarisation, so focusing the window then would replace
    // "Saving…" with an enabled Start button and open a second recorder.
    const poll = (phase: MeetingPhase | null) =>
      setLive((current) =>
        current?.phase.phase === "saving" && !phase
          ? current
          : phase && phase.phase !== "idle"
            ? { phase, at: Date.now() }
            : null,
      );

    const refresh = () => {
      or(commands.recentNotes(20), [], "loading meeting notes").then(setPast);
      commands
        .meetingStatus()
        .then(poll)
        .catch((e) => console.error(`reading meeting status: ${e}`));
    };
    refresh();
    window.addEventListener("focus", refresh);

    const unlisten = events.meetingPhase.listen(({ payload }) => {
      arrive(payload);
      // The note is written as the meeting goes idle, not before.
      if (payload.phase === "idle") refresh();
    });
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      window.removeEventListener("focus", refresh);
      unlisten.then((off) => off());
      clearInterval(timer);
    };
  }, []);

  if (open)
    return (
      <NoteReader
        note={open}
        onBack={() => setOpen(null)}
        backLabel="Meetings"
      />
    );

  const recording = live?.phase.phase === "recording" ? live.phase : null;
  const saving = live?.phase.phase === "saving";
  const elapsed = recording
    ? recording.elapsedSecs + Math.max(0, Math.floor((now - live!.at) / 1000))
    : 0;

  return (
    <div className="flex min-h-0 flex-col gap-5">
      <PaneTitle
        title="Meetings"
        subtitle="Ambient capture turns a meeting into a summarised note. Start it here, from the menu bar, or with the Meeting Notes hotkey."
      />

      <Surface className="flex items-center justify-between gap-4 p-5">
        {recording ? (
          <div className="flex items-center gap-3">
            <span className="size-[9px] shrink-0 animate-pulse rounded-full bg-brand" />
            <div>
              <p className="text-lg font-semibold tabular-nums">
                {formatElapsed(elapsed)}
              </p>
              <p className="text-ui text-ink-2">
                {recording.words} {recording.words === 1 ? "word" : "words"} so far
              </p>
            </div>
          </div>
        ) : (
          <p className="text-ui text-ink-2">
            {saving ? "Summarising and saving the note…" : "No meeting running."}
          </p>
        )}
        <Button
          variant="accent"
          size="row"
          className="shrink-0 whitespace-nowrap"
          disabled={saving}
          onClick={() => commands.toggleMeeting()}
        >
          {recording ? "Stop and Save" : "Start Meeting"}
        </Button>
      </Surface>

      <Group title="Past Meetings" className="min-h-0 overflow-y-auto">
        {past.length === 0 ? (
          <p className="py-8 text-center text-ui text-ink-2">No meetings yet.</p>
        ) : (
          past.map((note) => (
            <GridRow key={note.path} columns="1fr auto" className="py-0">
              <button
                onClick={() => setOpen(note)}
                className="col-span-2 grid grid-cols-subgrid items-center py-3 text-left"
              >
                <span className="truncate text-row font-medium">
                  {note.title}
                </span>
                <span className="shrink-0 whitespace-nowrap text-hint text-ink-2">
                  {formatDate(note.date)}
                </span>
              </button>
            </GridRow>
          ))
        )}
      </Group>
    </div>
  );
}
