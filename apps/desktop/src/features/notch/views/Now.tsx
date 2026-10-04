import { Button } from "@/components/ui/button";
import { Empty, Label, List, Row } from "@/components/notch/list";
import { IslandIcon } from "../IslandPill";
import { useNotch } from "../context";

/**
 * Now › Overview: everything live, in the order the closed notch ranks it,
 * each with the one thing to do about it. The glance, one step deeper.
 */
export function NowOverview() {
  const { islands, sessions, permissions, act } = useNotch();
  const live = islands.filter((island) => island.kind === "live");

  if (live.length === 0) {
    return (
      <Empty title="Nothing live right now">
        Coding agents show up here while they work or wait for you, and beside
        the camera when the notch is closed.
      </Empty>
    );
  }

  return (
    <>
      <Label>Live now</Label>
      <List label="Live activities">
        {live.map((island) => {
          const action = island.actions[0];
          return (
            <Row
              key={island.id}
              lead={
                <IslandIcon island={island} sessions={sessions} permissions={permissions} size="md" />
              }
              title={island.title}
              subtitle={island.detail}
              trailing={
                action && (
                  // What needs the user gets the panel's one inverted button.
                  <Button
                    variant={island.priority === "urgent" ? "notch" : "notch-quiet"}
                    size="notch"
                    onClick={() => void act(action)}
                  >
                    {action.label}
                  </Button>
                )
              }
            />
          );
        })}
      </List>
    </>
  );
}
