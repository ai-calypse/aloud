import { cn } from "@/lib/utils";
import type { EventTone } from "./roles";

/**
 * A calendar event as Apple draws it: a 3px bar down the left, the title in
 * the calendar's light colour, the time under it in a dimmer one, all on a
 * deep fill of the same hue. The words carry the event; the colour only says
 * which calendar it is from.
 */
export function EventCard({
  title,
  time,
  tone,
  className,
}: {
  title: string;
  /** "2:30–3:00 PM", "now", "Home": whatever sits under the title. */
  time?: string;
  tone: EventTone;
  className?: string;
}) {
  return (
    <div
      data-w-event={tone}
      className={cn(
        "grid min-w-0 grid-cols-[3px_1fr] gap-1.5 rounded-notch-w-card bg-widget-ev-bg py-1 pr-1.5 pl-1",
        // The bar is the card's own first column, so it grows with the card.
        "before:rounded-full before:bg-widget-ev-bar before:content-['']",
        className,
      )}
    >
      <div className="min-w-0">
        <div className="truncate text-notch-w-title leading-tight font-semibold text-widget-ev-title">
          {title}
        </div>
        {time && (
          <div className="truncate text-notch-w-meta leading-tight font-medium text-widget-ev-time">
            {time}
          </div>
        )}
      </div>
    </div>
  );
}
