import { commands } from "@/lib/bindings";
import { Empty, Label, Lead, List, Row, Value } from "@/components/notch/list";
import { Bar } from "@/components/notch/meters";
import { usePoll } from "../usePoll";
import { TONE_TEXT, formatGb, formatRate, num, thermalReading, usageTone } from "./mac-format";

/** Two seconds: a number that moves, without the Mac spending its own load drawing it. */
const POLL_MS = 2000;

/**
 * Mac › Stats: the CPU leads, the rest sit on rows. Reads only while this
 * view is on screen (usePoll lives and dies with it).
 */
export function MacStats() {
  const stats = usePoll(async () => {
    const res = await commands.macStats();
    if (res.status === "error") throw new Error(res.error);
    return res.data;
  }, POLL_MS);

  if (!stats) {
    return (
      <Empty title="Reading the Mac">
        CPU, memory, disk and network show up here a moment after the panel opens.
      </Empty>
    );
  }

  const cpu = num(stats.cpu);
  const memory = pct(num(stats.memoryUsedGb), num(stats.memoryTotalGb));
  const disk = pct(num(stats.diskUsedGb), num(stats.diskTotalGb));
  const thermal = thermalReading(stats.thermal);

  return (
    <div className="grid grid-cols-[1fr_1.4fr] gap-6 font-notch">
      <div>
        <Label>CPU</Label>
        <Lead sub="All cores">{Math.round(cpu)}%</Lead>
        <Bar className="mt-2.5" value={cpu / 100} tone={usageTone(cpu)} label="CPU" />
      </div>
      <List label="System">
        <Row
          title="Memory"
          subtitle={`${formatGb(num(stats.memoryUsedGb))} of ${formatGb(num(stats.memoryTotalGb))}`}
          trailing={<Meter value={memory} label="Memory" />}
        />
        <Row
          title="Disk"
          subtitle={`${formatGb(num(stats.diskTotalGb) - num(stats.diskUsedGb))} free`}
          trailing={<Meter value={disk} label="Disk" />}
        />
        <Row
          title="Network"
          subtitle={`↑ ${formatRate(stats.netUp)}`}
          trailing={<Value>↓ {formatRate(stats.netDown)}</Value>}
        />
        <Row
          title="Heat"
          subtitle="Thermal pressure"
          trailing={
            <Value className={TONE_TEXT[thermal.tone]}>{thermal.word}</Value>
          }
        />
      </List>
    </div>
  );
}

function pct(used: number, total: number): number {
  return total > 0 ? (used / total) * 100 : 0;
}

/** A small bar beside its number: the tone never stands alone. */
function Meter({ value, label }: { value: number; label: string }) {
  return (
    <>
      <Bar className="w-20" value={value / 100} tone={usageTone(value)} label={label} />
      <Value className="w-9 text-right">{Math.round(value)}%</Value>
    </>
  );
}
