import { useCallback, useEffect, useState } from "react";
import { commands, events, type Connector } from "../../lib/bindings";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";

/**
 * Connectors: the services the agent can act through, and a search to add more.
 *
 * Everything here comes from Composio at runtime rather than a hardcoded list,
 * so connecting a new service in the browser makes its tools available to the
 * agent without a release. The tool rows are the real set the model is offered,
 * not a description of it — if a tool is not listed, the agent cannot call it.
 */
export function ConnectorEditor() {
  const [connected, setConnected] = useState<Connector[] | null>(null);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Connector[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const result = await commands.listConnectors();
    if (result.status === "error") {
      setConnected([]);
      setError(result.error);
      return;
    }
    setError(null);
    setConnected(result.data);
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
    setBusy(connector.slug);
    const result = await commands.disconnectConnector(connector.connectionId);
    if (result.status === "error") setError(result.error);
    setBusy(null);
    refresh();
  }

  // Anything already connected is shown above, so it would only be noise here.
  const connectedSlugs = new Set((connected ?? []).map((c) => c.slug));
  const addable = results.filter((r) => !connectedSlugs.has(r.slug));

  return (
    <div className="flex flex-col gap-5">
      {error && (
        <p className="m-0 rounded-lg bg-[var(--warning-bg)] px-3 py-2 text-xs text-[var(--warning)]">
          {error}
        </p>
      )}

      <section className="flex flex-col gap-3">
        <h3 className="m-0 ml-1 text-xs font-semibold uppercase tracking-[0.03em] text-[var(--text2)]">Connected</h3>
        {connected === null ? (
          <p className="m-0 text-sm text-muted-foreground">Loading…</p>
        ) : connected.length === 0 ? (
          <p className="m-0 text-sm text-muted-foreground">
            Nothing connected yet. Search below to add Gmail, GitHub, Slack, or
            anything else Composio supports.
          </p>
        ) : (
          connected.map((c) => (
            <article
              key={c.slug}
              className="rounded-[11px] border-[0.5px] border-[var(--card-border)] bg-[var(--card-solid)] shadow-[0_1px_2px_rgba(0,0,0,.05)] p-4"
            >
              <header className="flex items-center gap-3">
                <Logo connector={c} />
                <div className="min-w-0 flex-1">
                  <p className="m-0 text-sm font-medium">{c.name}</p>
                  <p className="m-0 truncate text-xs text-muted-foreground">
                    {c.toolsCount} tools available · {c.tools.length} offered to
                    the agent
                  </p>
                </div>
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={busy === c.slug}
                  onClick={() => disconnect(c)}
                >
                  Disconnect
                </Button>
              </header>

              {c.tools.length > 0 && (
                <ul className="m-0 mt-3 flex list-none flex-col gap-1.5 p-0">
                  {c.tools.map((t) => (
                    <li
                      key={t.name}
                      className="flex items-center gap-2 text-xs"
                      title={t.description}
                    >
                      <code className="min-w-0 truncate text-muted-foreground">
                        {t.name}
                      </code>
                      {!t.readOnly && (
                        <Badge variant="secondary" className="shrink-0">
                          asks first
                        </Badge>
                      )}
                    </li>
                  ))}
                </ul>
              )}
            </article>
          ))
        )}
      </section>

      <section className="flex flex-col gap-3">
        <h3 className="m-0 ml-1 text-xs font-semibold uppercase tracking-[0.03em] text-[var(--text2)]">Add a connector</h3>
        <Input
          type="search"
          value={query}
          placeholder="Search Slack, Notion, Linear, Google Drive…"
          onChange={(e) => setQuery(e.target.value)}
        />

        {addable.length === 0 ? (
          <p className="m-0 text-sm text-muted-foreground">
            {query.trim() ? `Nothing matches “${query}”.` : "Loading…"}
          </p>
        ) : (
          <ul className="m-0 flex list-none flex-col gap-2 p-0">
            {addable.map((c) => (
              <li
                key={c.slug}
                className="flex items-center gap-3 rounded-[11px] border-[0.5px] border-[var(--card-border)] bg-[var(--card-solid)] shadow-[0_1px_2px_rgba(0,0,0,.05)] p-3"
              >
                <Logo connector={c} />
                <div className="min-w-0 flex-1">
                  <p className="m-0 text-sm font-medium">{c.name}</p>
                  <p className="m-0 line-clamp-2 text-xs text-muted-foreground">
                    {c.description || `${c.toolsCount} tools`}
                  </p>
                </div>
                <Button
                  size="sm"
                  disabled={busy === c.slug}
                  onClick={() => connect(c.slug)}
                >
                  {busy === c.slug ? "Opening…" : "Connect"}
                </Button>
              </li>
            ))}
          </ul>
        )}
        <p className="m-0 text-xs text-muted-foreground">
          Connecting opens Composio's consent page in your browser. Come back to
          this window when you are done and the connector appears above.
        </p>
      </section>
    </div>
  );
}

/** The toolkit's logo, falling back to its initial when the URL fails. */
function Logo({ connector }: { connector: Connector }) {
  const [failed, setFailed] = useState(false);

  if (!connector.logo || failed) {
    return (
      <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted text-sm font-semibold">
        {connector.name.charAt(0).toUpperCase()}
      </span>
    );
  }
  return (
    <img
      src={connector.logo}
      alt=""
      className="size-8 shrink-0 rounded-lg object-contain"
      onError={() => setFailed(true)}
    />
  );
}
