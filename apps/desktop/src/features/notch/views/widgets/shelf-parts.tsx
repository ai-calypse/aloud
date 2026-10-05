import { useState, type ButtonHTMLAttributes, type ReactNode } from "react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { Switch } from "@/components/notch/controls";
import { WidgetFrame, type WidgetSize } from "@/components/notch/widgets";
import type { ShelfWatch } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { useThumb } from "../shelf-data";

/**
 * The pieces the four Shelf boards share: buttons that read as widget
 * controls, a quiet status line, a tile for "nothing here yet", and the
 * on/off tile for a folder watcher. Behaviour is the shelf's own; only the
 * look is new.
 */

const PILL = {
  // White with dark words, as Home's play pill: Apple's translucent one is under 3:1.
  primary: "bg-widget-ink text-widget-on",
  quiet: "bg-widget-ink/15 text-widget-ink",
  // Red words through the role system, so the accent rule keeps them at AA.
  danger: "bg-widget-ink/15 text-widget-acc-ink",
} as const;

export function Pill({
  tone = "quiet",
  className,
  ...props
}: { tone?: keyof typeof PILL } & ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      data-w-role={tone === "danger" ? "move" : undefined}
      className={cn(
        NOTCH_FOCUS,
        "flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-full px-3 text-notch-w-meta font-semibold whitespace-nowrap",
        "transition-opacity duration-[var(--dur-fast)] ease-[var(--ease-out)] hover:opacity-85 disabled:opacity-40 [&_svg]:size-3.5",
        PILL[tone],
        className,
      )}
      {...props}
    />
  );
}

/** A round control that sits on a picture: solid, so it reads over any screenshot. */
export function Chip({ className, ...props }: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      className={cn(
        NOTCH_FOCUS,
        "grid size-7 place-items-center rounded-full bg-widget-tile/85 text-widget-ink [&_svg]:size-3.5",
        "transition-opacity duration-[var(--dur-fast)] ease-[var(--ease-out)] hover:bg-widget-tile",
        className,
      )}
      {...props}
    />
  );
}

/** What went wrong, or what just happened, under a board. */
export function Note({ children }: { children: ReactNode }) {
  return (
    <p role="status" className="m-0 mt-2 text-center text-notch-w-meta text-widget-ink-2">
      {children}
    </p>
  );
}

/** A picture's thumbnail, or its kind's glyph until (or instead of) one. */
export function ShelfThumb({
  path,
  image,
  glyph,
  className,
}: {
  path: string;
  image: boolean;
  glyph: ReactNode;
  /** The box: size and radius. */
  className?: string;
}) {
  const src = useThumb(path, image);
  return src ? (
    <img src={src} alt="" draggable={false} className={cn("shrink-0 object-cover", className)} />
  ) : (
    <span
      aria-hidden
      className={cn("grid shrink-0 place-items-center text-widget-ink-2 [&_svg]:size-1/2", className)}
    >
      {glyph}
    </span>
  );
}

/**
 * A tile that says what fills it and, if there is one, the single thing to
 * do: the widget form of the calm `Empty`. Never a bare "nothing here".
 */
export function InfoTile({
  size = "medium",
  label,
  icon,
  title,
  children,
  action,
  ground,
}: {
  size?: WidgetSize;
  label: string;
  icon: ReactNode;
  title: ReactNode;
  children?: ReactNode;
  action?: ReactNode;
  ground?: "tile" | "vibrant" | "deep";
}) {
  return (
    <WidgetFrame size={size} label={label} ground={ground}>
      <div className="flex h-full flex-col justify-between gap-2 p-3.5">
        <div>
          <div className="flex items-center gap-2 text-widget-ink">
            <span aria-hidden className="text-widget-ink-2 [&_svg]:size-5">
              {icon}
            </span>
            <span className="truncate text-notch-w-headline leading-tight font-semibold">{title}</span>
          </div>
          {children && (
            <p className="m-0 mt-1.5 text-notch-w-meta leading-snug text-pretty text-widget-ink-2">{children}</p>
          )}
        </div>
        {action}
      </div>
    </WidgetFrame>
  );
}

type OnSet = (which: ShelfWatch, on: boolean) => Promise<string | null>;

/**
 * A folder watcher that is off: what it would do, and the one button. One
 * that cannot start (macOS has not allowed the folder yet) says why and
 * stays off.
 */
export function TurnOnTile({
  label,
  icon,
  title,
  children,
  which,
  onSet,
}: {
  label: string;
  icon: ReactNode;
  title: string;
  children: ReactNode;
  which: ShelfWatch;
  onSet: OnSet;
}) {
  const [error, setError] = useState<string | null>(null);
  return (
    <InfoTile
      label={label}
      icon={icon}
      title={title}
      action={
        <div className="flex items-center gap-2.5">
          <Pill tone="primary" onClick={() => void onSet(which, true).then(setError)}>
            Turn on
          </Pill>
          {error && (
            <span role="status" data-w-role="move" className="min-w-0 text-notch-w-meta leading-tight text-widget-acc-ink">
              {error}
            </span>
          )}
        </div>
      }
    >
      {children}
    </InfoTile>
  );
}

/**
 * The view's on/off tile: whether Aloud is watching this folder at all. A
 * failure to start shows in the tile and the switch stays where Rust left it.
 */
export function WatchTile({
  which,
  on,
  title,
  subtitle,
  icon,
  onSet,
}: {
  which: ShelfWatch;
  on: boolean;
  title: string;
  subtitle: string;
  icon: ReactNode;
  onSet: OnSet;
}) {
  const [error, setError] = useState<string | null>(null);
  return (
    <WidgetFrame size="small" label={title}>
      <div className="flex h-full flex-col justify-between p-3.5">
        <span aria-hidden className="text-widget-ink-2 [&_svg]:size-6">
          {icon}
        </span>
        <div className="flex flex-col gap-2">
          {error ? (
            <span role="status" data-w-role="move" className="text-notch-w-meta leading-snug text-widget-acc-ink">
              {error}
            </span>
          ) : (
            <span className="text-notch-w-meta leading-snug text-widget-ink-2">{subtitle}</span>
          )}
          <Switch label={title} checked={on} onChange={(next) => void onSet(which, next).then(setError)} />
        </div>
      </div>
    </WidgetFrame>
  );
}
