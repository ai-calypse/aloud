import { useCallback, useEffect, useMemo, useState } from "react";
import { commands, events, type Connector } from "../../lib/bindings";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/**
 * Connectors: the services the agent can act through, as one list of services
 * rather than a catalogue of tool names.
 *
 * Everything here comes from Composio at runtime rather than a hardcoded list,
 * so connecting a new service in the browser makes its tools available to the
 * agent without a release.
 *
 * Deliberately says nothing about individual tools. Selection is per command —
 * every tool in a connected toolkit is a candidate on every request — so any
 * list shown here would be a sample masquerading as a promise. Listing services
 * is the honest granularity, and it costs one request instead of one per
 * connected service.
 */

type Filter = "all" | "connected" | "available";

const FILTERS: { id: Filter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "connected", label: "Connected" },
  { id: "available", label: "Not connected" },
];

/** Cards above the table. The catalogue's own head, minus what is connected. */
const POPULAR_COUNT = 3;

export function ConnectorEditor() {
  const [connected, setConnected] = useState<Connector[] | null>(null);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Connector[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  /** The connector awaiting a disconnect confirmation, if any. */
  const [confirming, setConfirming] = useState<Connector | null>(null);
  /**
   * Slugs the user has disconnected, held until Composio agrees.
   *
   * A DELETE followed straight away by a GET can still report the account as
   * ACTIVE, which would flip the row back to connected a second after the user
   * disconnected it — the same confusion the optimistic update exists to avoid.
   */
  const [disconnected, setDisconnected] = useState<Set<string>>(new Set());

  const refresh = useCallback(async () => {
    const result = await commands.listConnectors();
    if (result.status === "error") {
      setConnected([]);
      setError(result.error);
      return;
    }
    setError(null);
    setConnected(result.data);
    // Drop the override once the server stops claiming a connection.
    setDisconnected((prev) => {
      if (prev.size === 0) return prev;
      const claimed = new Set(
        result.data.filter((c) => c.connected).map((c) => c.slug),
      );
      return new Set([...prev].filter((slug) => claimed.has(slug)));
    });
  }, []);

  useEffect(() => {
    refresh();
    // OAuth finishes in the browser, so the new account only shows up once we
    // come back to this window.
    window.addEventListener("focus", refresh);
    // `refresh` answers from cache and revalidates behind it, so the fresh
    // list arrives here rather than as the return value. Without this listener
    // a service connected in the browser would never appear.
    const unlisten = events.connectorsUpdated.listen(({ payload }) => {
      setError(null);
      setConnected(payload);
    });
    return () => {
      window.removeEventListener("focus", refresh);
      unlisten.then((off) => off());
    };
  }, [refresh]);

  useEffect(() => {
    // Composio is a network round-trip per keystroke otherwise.
    const timer = setTimeout(async () => {
      const result = await commands.searchConnectors(query);
      setResults(result.status === "ok" ? result.data : []);
    }, 200);
    return () => clearTimeout(timer);
  }, [query]);

  async function connect(slug: string) {
    setBusy(slug);
    const result = await commands.connectConnector(slug);
    if (result.status === "error") setError(result.error);
    setBusy(null);
  }

  async function disconnect(connector: Connector) {
    if (!connector.connectionId) return;
    setConfirming(null);
    setBusy(connector.slug);
    const result = await commands.disconnectConnector(connector.connectionId);
    setBusy(null);
    if (result.status === "error") {
      setError(result.error);
      return;
    }

    // Flip the row here rather than waiting for the refetch below. Disconnect
    // clears the backend cache, so that refetch is the cold path — a full
    // Composio round-trip during which the row would otherwise keep its tick
    // and read as "nothing happened".
    setDisconnected((prev) => new Set(prev).add(connector.slug));
    refresh();
  }

  // One list: connected services first, then whatever the search offers. The
  // connected copy wins on slug, since only it carries the connectionId needed
  // to disconnect.
  const rows = useMemo(() => {
    const mine = (connected ?? []).map((c) =>
      disconnected.has(c.slug)
        ? { ...c, connected: false, connectionId: null }
        : c,
    );
    const slugs = new Set(mine.map((c) => c.slug));
    const merged = [...mine, ...results.filter((r) => !slugs.has(r.slug))];
    if (filter === "connected") return merged.filter((c) => c.connected);
    if (filter === "available") return merged.filter((c) => !c.connected);
    return merged;
  }, [connected, results, filter, disconnected]);

  // Only while browsing: once the user types, they want the table, not a
  // shortlist chosen for them. Hidden under "Connected" for the same reason.
  //
  // Drawn from the catalogue, never from the user's own list: promoting a
  // just-disconnected service into a "Popular" card would yank the row the
  // user was looking at out of the table and into a promo strip.
  const mineSlugs = new Set((connected ?? []).map((c) => c.slug));
  const browsing = !query.trim() && filter !== "connected";
  const popular = browsing
    ? results
        .filter((c) => !c.connected && !mineSlugs.has(c.slug))
        .slice(0, POPULAR_COUNT)
    : [];
  // Promoted into cards, so listing them again below is noise.
  const promoted = new Set(popular.map((c) => c.slug));
  const tableRows = rows.filter((c) => !promoted.has(c.slug));

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between gap-4">
        <h1 className="text-2xl font-bold leading-[1.1] tracking-[-0.02em]">
          Connectors
        </h1>
        <div className="flex w-[260px] items-center gap-[7px] rounded-field border-[0.5px] border-surface-border bg-surface px-3 py-1.5 shadow-field focus-within:border-brand">
          <svg
            viewBox="0 0 16 16"
            className="size-[13px] shrink-0 text-search-icon"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.6"
          >
            <circle cx="7" cy="7" r="4.2" />
            <line x1="10.2" y1="10.2" x2="13.5" y2="13.5" strokeLinecap="round" />
          </svg>
          <input
            type="search"
            className="w-full bg-transparent text-ui outline-none placeholder:text-ink-2"
            placeholder="Search Slack, Notion, Linear…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
      </div>

      {error && (
        <p className="m-0 rounded-lg bg-warning-bg px-3 py-2 text-xs text-warning">
          {error}
        </p>
      )}

      <div className="flex gap-1">
        {FILTERS.map((f) => (
          <button
            key={f.id}
            onClick={() => setFilter(f.id)}
            className={cn(
              "rounded-lg px-3 py-1.5 text-ui font-medium transition-colors",
              filter === f.id
                ? "bg-field text-foreground"
                : "text-ink-2 hover:text-foreground",
            )}
          >
            {f.label}
          </button>
        ))}
      </div>

      {popular.length > 0 && (
        <section>
          <h3 className="mb-[7px] ml-1 text-xs font-semibold uppercase tracking-[0.03em] text-ink-2">
            Popular
          </h3>
          <div className="grid grid-cols-3 gap-3">
            {popular.map((c) => (
              <div
                key={c.slug}
                className="flex items-center gap-2 rounded-card border-[0.5px] border-surface-border bg-surface p-2.5 shadow-card"
              >
                <Logo connector={c} small />
                {/* Three-up in a 686px pane, so a long name ellipsizes —
                    the title keeps it recoverable. */}
                <span
                  title={c.name}
                  className="min-w-0 flex-1 truncate text-ui font-medium"
                >
                  {c.name}
                </span>
                <ConnectButton
                  busy={busy === c.slug}
                  onClick={() => connect(c.slug)}
                  compact
                />
              </div>
            ))}
          </div>
        </section>
      )}

      <section>
        <div className="grid grid-cols-[1fr_90px_110px] items-center gap-4 border-b-[0.5px] border-separator px-1 pb-2 text-ui font-medium text-ink-2">
          <span>Connector</span>
          <span>Type</span>
          <span>Status</span>
        </div>

        {connected === null ? (
          <p className="px-1 py-6 text-ui text-ink-2">Loading…</p>
        ) : tableRows.length === 0 ? (
          <p className="px-1 py-6 text-ui text-ink-2">
            {query.trim()
              ? `Nothing matches “${query}”.`
              : "Nothing connected yet. Search to add Gmail, GitHub, Slack, or anything else Composio supports."}
          </p>
        ) : (
          tableRows.map((c) => (
            <div
              key={c.slug}
              className="group grid grid-cols-[1fr_90px_110px] items-center gap-4 border-b-[0.5px] border-separator px-1 py-2.5 last:border-b-0 hover:bg-hover"
            >
              <div className="flex min-w-0 items-center gap-2.5">
                <Logo connector={c} />
                <span title={c.name} className="truncate text-sm">
                  {c.name}
                </span>
              </div>
              <span className="text-ui text-ink-2">Web</span>
              {c.connected ? (
                <div className="flex items-center gap-3">
                  <svg
                    viewBox="0 0 16 16"
                    className="size-4 shrink-0 text-ink-2"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.8"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    aria-label="Connected"
                    role="img"
                  >
                    <path d="M3 8.5l3.5 3.5L13 5" />
                  </svg>
                  {/* Hidden until hover so a connected row stays quiet, but
                      focusable so it is reachable without a mouse. */}
                  <button
                    className="text-ui font-medium text-danger opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 disabled:opacity-50"
                    disabled={busy === c.slug}
                    onClick={() => setConfirming(c)}
                  >
                    Disconnect
                  </button>
                </div>
              ) : (
                <ConnectButton
                  busy={busy === c.slug}
                  onClick={() => connect(c.slug)}
                />
              )}
            </div>
          ))
        )}
      </section>

      <p className="m-0 text-xs text-ink-2">
        Connecting opens Composio's consent page in your browser. Come back to
        this window when you are done and the connector appears here.
      </p>

      {/* Disconnecting revokes the account and cannot be undone from here —
          reconnecting means going through OAuth again. */}
      <Dialog
        open={confirming !== null}
        onOpenChange={(open) => !open && setConfirming(null)}
      >
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Disconnect {confirming?.name}?</DialogTitle>
          </DialogHeader>
          <p className="m-0 text-sm text-muted-foreground">
            The agent will no longer be able to act through{" "}
            {confirming?.name ?? "this service"}. You can reconnect later, but
            you will have to sign in again.
          </p>
          <DialogFooter className="justify-between sm:justify-between">
            <Button variant="secondary" onClick={() => setConfirming(null)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              disabled={busy === confirming?.slug}
              onClick={() => confirming && disconnect(confirming)}
            >
              Disconnect
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function ConnectButton({
  busy,
  onClick,
  compact,
}: {
  busy: boolean;
  onClick: () => void;
  /** Tighter, for the Popular cards where three sit side by side. */
  compact?: boolean;
}) {
  return (
    <button
      disabled={busy}
      onClick={onClick}
      className={cn(
        "shrink-0 rounded-lg bg-field font-medium transition-colors hover:bg-brand hover:text-white disabled:opacity-50",
        compact ? "px-2.5 py-1 text-xs" : "px-3 py-1.5 text-ui",
      )}
    >
      {busy ? "Opening…" : "Connect"}
    </button>
  );
}

/** The toolkit's logo, falling back to its initial when the URL fails. */
function Logo({
  connector,
  small,
}: {
  connector: Connector;
  /** Three cards share one row in a 686px pane; every pixel goes to the name. */
  small?: boolean;
}) {
  const [failed, setFailed] = useState(false);
  const size = small ? "size-6" : "size-7";

  if (!connector.logo || failed) {
    return (
      <span
        className={cn(
          "flex shrink-0 items-center justify-center rounded-lg bg-field text-xs font-semibold",
          size,
        )}
      >
        {connector.name.charAt(0).toUpperCase()}
      </span>
    );
  }
  return (
    <img
      src={connector.logo}
      alt=""
      className={cn("shrink-0 rounded-lg object-contain", size)}
      onError={() => setFailed(true)}
    />
  );
}
