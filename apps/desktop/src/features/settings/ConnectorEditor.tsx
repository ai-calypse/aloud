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
import { SearchField } from "@/components/ui/search-field";
import { Caption, PaneTitle, Surface } from "@/components/ui/rows";
import { cn } from "@/lib/utils";

/**
 * The services the agent can act through, listed from Composio at runtime so a
 * newly connected service needs no release.
 *
 * Deliberately says nothing about individual tools: every tool in a connected
 * toolkit is a candidate on every request, so any list here would be a sample
 * masquerading as a promise — and it costs one request, not one per service.
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
  /** Slugs the user has disconnected, held until Composio agrees: a DELETE
      followed straight away by a GET can still report the account ACTIVE, and
      the row would flip back to connected a second later. */
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
    // list arrives on this event rather than as the return value.
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

    // Disconnect clears the backend cache, so the refetch below is a full cold
    // round-trip; without this the row keeps its tick throughout.
    setDisconnected((prev) => new Set(prev).add(connector.slug));
    refresh();
  }

  // Connected first, then search results. The connected copy wins on slug —
  // only it carries the connectionId needed to disconnect.
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

  // Only while browsing, and drawn from the catalogue rather than the user's
  // own list: promoting a just-disconnected service into a Popular card would
  // yank the row the user was looking at out of the table.
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
        <PaneTitle title="Connectors" />
        <SearchField
          className="w-[260px]"
          value={query}
          onChange={setQuery}
          placeholder="Search Slack, Notion, Linear…"
        />
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
          <Caption>Popular</Caption>
          <div className="grid grid-cols-3 gap-3">
            {popular.map((c) => (
              <Surface
                key={c.slug}
                className="flex items-center gap-2 p-2.5"
              >
                <Logo connector={c} small />
                {/* Three-up in a 686px pane: long names ellipsize, so the
                    title attribute keeps them recoverable. */}
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
              </Surface>
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

      {/* Confirmed because disconnecting revokes the account: reconnecting
          means going through OAuth again. */}
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
    <Button
      variant="field"
      disabled={busy}
      onClick={onClick}
      className={cn(
        "shrink-0",
        compact ? "px-2.5 py-1 text-xs" : "px-3 py-1.5 text-ui",
      )}
    >
      {busy ? "Opening…" : "Connect"}
    </Button>
  );
}

/** The toolkit's logo, falling back to its initial when the URL fails. */
function Logo({
  connector,
  small,
}: {
  connector: Connector;
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
