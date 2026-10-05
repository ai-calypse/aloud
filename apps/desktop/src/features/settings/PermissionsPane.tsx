import { useEffect, useState } from "react";
import { commands, type Grant, type PermissionRow } from "@/lib/bindings";
import { or } from "@/lib/result";
import { Group, PaneTitle, Row, Value, rowSelect } from "@/components/ui/rows";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const LABEL: Record<Grant, string> = {
  always: "Always allow",
  ask: "Ask",
  never: "Never",
};

/** "Always allow" is not offered where it could never apply. */
const ALL: Grant[] = ["always", "ask", "never"];
const STRICT: Grant[] = ["ask", "never"];

/**
 * What Aloud may do in each app without asking: one group per app, one row per
 * action. The same grants an approval card's "Always allow" saves.
 */
export function PermissionsPane() {
  const [rows, setRows] = useState<PermissionRow[]>([]);

  useEffect(() => {
    void or(commands.listPermissions(), [], "reading permissions").then(setRows);
  }, []);

  const set = (row: PermissionRow, grant: Grant) => {
    setRows((current) => current.map((r) => (r.key === row.key ? { ...r, grant } : r)));
    void or(
      commands.setPermission(row.key, row.app, row.action, grant),
      null,
      "saving a permission",
    );
  };

  const apps = new Map<string, PermissionRow[]>();
  for (const row of [...rows].sort((a, b) => a.app.localeCompare(b.app))) {
    apps.set(row.app, [...(apps.get(row.app) ?? []), row]);
  }

  return (
    <>
      <PaneTitle
        title="Permissions"
        subtitle="What Aloud may do in each app without asking. Paying, permanent deletes, public posts and passwords always ask."
      />
      {[...apps].map(([app, list]) => (
        <Group key={app} title={app}>
          {list.map((row) => (
            <Row key={row.key} label={row.action.charAt(0).toUpperCase() + row.action.slice(1)}>
              <Value>
                <Select value={row.grant} onValueChange={(v) => set(row, v as Grant)}>
                  <SelectTrigger className={rowSelect}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {(row.strict ? STRICT : ALL).map((grant) => (
                      <SelectItem key={grant} value={grant}>
                        {LABEL[grant]}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Value>
            </Row>
          ))}
        </Group>
      ))}
    </>
  );
}
