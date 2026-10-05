import React, {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { commands, type Agent as CodingAgent, type LocalRepo } from "@/lib/bindings";
import { Shimmer } from "@/components/ai-elements/shimmer";
import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { ChevronsUpDownIcon } from "lucide-react";
import { Panel } from "./Panel";

/** Where the last-used repo path is remembered between sessions, so the
 *  common case (same repo again) is one field pre-filled rather than retyped.
 *  ponytail: one string in localStorage, not a recent-repos list — add the
 *  list if people work across many repos often enough to miss it. */
const LAST_REPO_KEY = "aloud.codingAgent.lastRepo";

/**
 * A searchable dropdown for the composer — a trigger plus a cmdk list, shared by
 * the Repo and Branch fields. The trigger wears the composer's field look
 * (`composer__trigger`); the list dark-renders for free through the overlay's
 * floating tokens. `allowCustom` lets Repo accept a typed path that isn't in
 * the list (a repo outside the scanned roots); Branch leaves it off.
 *
 * The open list renders **inline**, not in a Radix Popover portal. The overlay
 * is a click-through window whose clickable region is exactly the panel box the
 * webview reports; a portal to `document.body` lands *outside* that box, so
 * every click on it fell through and dismissed the notch. Rendered inline, the
 * list is part of the panel's measured content — `useNotchPanel` grows the box
 * to include it, so its clicks land inside and it stops closing the notch.
 */
const Combobox = React.forwardRef<
  HTMLButtonElement,
  {
    value: string;
    onChange: (value: string) => void;
    options: string[];
    placeholder: string;
    empty: string;
    disabled?: boolean;
    allowCustom?: boolean;
    /** Show a shimmer in the trigger instead of a value, while options load. */
    loading?: boolean;
  }
>(function Combobox(
  { value, onChange, options, placeholder, empty, disabled, allowCustom, loading },
  triggerRef,
) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");

  const pick = (v: string) => {
    onChange(v);
    setOpen(false);
    setQuery("");
  };

  return (
    <div className="composer__combobox">
      <button
        ref={triggerRef}
        className="composer__trigger"
        type="button"
        disabled={disabled}
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        {loading ? (
          <Shimmer as="span">Loading…</Shimmer>
        ) : (
          <span className={value ? undefined : "composer__trigger-placeholder"}>
            {value || placeholder}
          </span>
        )}
        <ChevronsUpDownIcon className="composer__trigger-icon" />
      </button>
      {open ? (
        <Command
          className="composer__list"
          // Let a typed path through even when it matches nothing in the list.
          shouldFilter={!allowCustom || options.length > 0}
          // Escape closes the list without closing the whole composer.
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.stopPropagation();
              setOpen(false);
            }
          }}
        >
          <CommandInput
            placeholder={placeholder}
            value={query}
            onValueChange={setQuery}
            autoFocus
          />
          <CommandList>
            <CommandEmpty>
              {allowCustom && query.trim() ? (
                <button className="composer__use-typed" onClick={() => pick(query.trim())}>
                  Use “{query.trim()}”
                </button>
              ) : (
                empty
              )}
            </CommandEmpty>
            {options.map((option) => (
              <CommandItem key={option} value={option} onSelect={() => pick(option)}>
                {option}
              </CommandItem>
            ))}
          </CommandList>
        </Command>
      ) : null}
    </div>
  );
});

/**
 * Agents › Coding's "New coding agent": the composer for starting one. Pick a repo,
 * pick one of its branches, say what to do, and Rust opens a Terminal tab that
 * checks that branch out in the repo and runs the agent on the task there (no
 * worktree — it edits the repo directly). The running session then shows up
 * in the list beside it through the same discovery every other session uses.
 *
 * The three choices are independent: the task is the agent's instruction
 * verbatim, and the branch is picked, not derived from it. Branches are fetched
 * when the repo settles, so the dropdown reflects the chosen repo.
 *
 * A `Panel` like `AgentRun`, so it inherits the close and the always-dark
 * chrome; the panel's own spring settles around the fields, so
 * revealing the branch row simply grows the notch — no animation of its own.
 */
export function CodingAgentComposer({
  bodyRef,
  onClose,
}: {
  bodyRef: React.RefObject<HTMLDivElement | null>;
  onClose: () => void;
}) {
  const [repo, setRepo] = useState(() => localStorage.getItem(LAST_REPO_KEY) ?? "");
  const [branch, setBranch] = useState("");
  const [task, setTask] = useState("");
  const [agent, setAgent] = useState<CodingAgent>("claude");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [started, setStarted] = useState<string | null>(null);
  const [hooksInstalled, setHooksInstalled] = useState<boolean | null>(null);
  const [repos, setRepos] = useState<LocalRepo[]>([]);
  // The chosen repo's branches, fetched when the repo settles. Empty until a
  // repo resolves; `loadingBranches` distinguishes "not fetched" from "none".
  const [branches, setBranches] = useState<string[]>([]);
  const [loadingBranches, setLoadingBranches] = useState(false);
  // The repo trigger, focused on open so the flow starts on the keyboard.
  const repoRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    void commands.agentHookStatus().then((res) => {
      setHooksInstalled(res.status === "ok" ? res.data.installed.length > 0 : true);
    });
    void commands.listLocalRepos().then((res) => {
      if (res.status === "ok") setRepos(res.data);
    });
    // DOM focus only — the overlay is already key (show_panel took focus).
    repoRef.current?.focus();
  }, []);

  // A picked name maps back to its path; anything else is a literal path (the
  // free-text escape hatch for a repo outside the scanned roots).
  const resolveRepo = useCallback(
    (value: string) => repos.find((r) => r.name === value)?.path ?? value,
    [repos],
  );

  // Fetch the repo's branches once it resolves to a real path. Keyed on the
  // resolved path so switching repos refetches; a stale response for a repo the
  // user has since changed away from is dropped by the `active` guard.
  const repoPath = repo.trim() ? resolveRepo(repo.trim()) : "";
  useEffect(() => {
    if (!repoPath) {
      setBranches([]);
      return;
    }
    let active = true;
    setLoadingBranches(true);
    void commands.listRepoBranches(repoPath).then((res) => {
      if (!active) return;
      setLoadingBranches(false);
      const list = res.status === "ok" ? res.data : [];
      setBranches(list);
      // Default to the first branch (most-recently-committed) so the common
      // case needs no branch click — but never clobber a choice already made.
      setBranch((b) => (b && list.includes(b) ? b : list[0] ?? ""));
    });
    return () => {
      active = false;
    };
  }, [repoPath]);

  const canStart = !busy && repoPath !== "" && branch !== "" && task.trim() !== "";

  const go = useCallback(() => {
    if (!canStart) return;
    setBusy(true);
    setError(null);
    localStorage.setItem(LAST_REPO_KEY, repo.trim());
    void commands.spawnCodingAgent(repoPath, branch, task.trim(), agent).then((res) => {
      setBusy(false);
      if (res.status === "ok") setStarted(res.data);
      else setError(res.error);
    });
  }, [canStart, repoPath, repo, branch, task, agent]);

  // "Start another" from the success state: keep the repo (you usually queue
  // several in the same one), clear the task, land focus back on the task.
  const startAnother = () => {
    setStarted(null);
    setTask("");
    setError(null);
  };

  return (
    <Panel title="New coding agent" bodyRef={bodyRef} onClose={onClose}>
      {started ? (
        <p className="ask__done">
          Started on <span className="ask__branch">{started}</span>. It will
          appear in the list in a moment.{" "}
          <button className="composer__link" onClick={startAnother}>
            Start another
          </button>
        </p>
      ) : (
        <div
          className="composer"
          // Cmd/Ctrl+Enter starts from anywhere in the composer, not just the
          // Task field — the whole form is one submit.
          onKeyDown={(e) => {
            if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
              e.preventDefault();
              go();
            }
          }}
        >
          <div className="composer__field">
            <span className="composer__label">Repo</span>
            <Combobox
              ref={repoRef}
              value={repo}
              onChange={setRepo}
              options={repos.map((r) => r.name)}
              placeholder={repos.length ? "Pick a repo" : "~/dev/my-app"}
              empty="No repos found"
              disabled={busy}
              // A repo outside the scanned roots can still be typed as a path.
              allowCustom
            />
          </div>

          {/* The branch row appears once a repo is chosen; the panel grows to
              fit it via useNotchPanel's spring — the reveal IS the motion. */}
          {repoPath ? (
            <div className="composer__field composer__field--branch">
              <span className="composer__label">Branch</span>
              <Combobox
                value={branch}
                onChange={setBranch}
                options={branches}
                placeholder="Pick a branch"
                empty="No branches found"
                loading={loadingBranches}
                disabled={busy || loadingBranches || branches.length === 0}
              />
            </div>
          ) : null}

          <label className="composer__field">
            <span className="composer__label">Task</span>
            <input
              className="composer__input"
              placeholder="What should it do?"
              value={task}
              disabled={busy}
              onChange={(e) => setTask(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && go()}
            />
          </label>

          <div className="composer__row">
            <div
              className="composer__agents"
              role="radiogroup"
              aria-label="Agent"
              // Arrow keys move between claude/codex, the radiogroup convention.
              onKeyDown={(e) => {
                if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
                  setAgent((a) => (a === "claude" ? "codex" : "claude"));
                }
              }}
            >
              {(["claude", "codex"] as const).map((option) => (
                <button
                  key={option}
                  className="composer__agent"
                  role="radio"
                  aria-checked={agent === option}
                  data-active={agent === option || undefined}
                  // Only the selected radio is tab-focusable; arrows move within.
                  tabIndex={agent === option ? 0 : -1}
                  disabled={busy}
                  onClick={() => setAgent(option)}
                >
                  {option}
                </button>
              ))}
            </div>
            <button className="composer__go" disabled={!canStart} onClick={go}>
              {busy ? "Starting…" : "Start"}
            </button>
          </div>

          {error ? <p className="ask__error">{error}</p> : null}
          {hooksInstalled === false ? (
            <p className="composer__hint">
              Approvals won't reach the notch until agent hooks are installed.{" "}
              <button
                className="composer__link"
                onClick={() => void commands.installAgentHooks().then(() => setHooksInstalled(true))}
              >
                Install hooks
              </button>
            </p>
          ) : null}
        </div>
      )}
    </Panel>
  );
}
