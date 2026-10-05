import type { ButtonHTMLAttributes, ReactNode } from "react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { WidgetFrame, type WidgetSize } from "@/components/notch/widgets";
import { commands, type AgendaError } from "@/lib/bindings";
import { cn } from "@/lib/utils";

/**
 * A white pill with dark words: the one action a tile is for (Join, Allow
 * access, Add). The same inverted treatment as Music's play pill, since
 * Apple's translucent one is under 3:1.
 */
export function Pill({
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      {...props}
      className={cn(
        NOTCH_FOCUS,
        "h-7 w-fit shrink-0 rounded-full bg-widget-ink px-3.5 text-notch-w-title font-semibold text-widget-on",
        "disabled:opacity-50",
        className,
      )}
    />
  );
}

/** Everything else a tile offers: words in the tile's secondary ink until pointed at. */
export function QuietPill({
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      {...props}
      className={cn(
        NOTCH_FOCUS,
        "h-7 shrink-0 rounded-full px-2.5 text-notch-w-title font-medium text-widget-ink-2",
        "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-out)] hover:bg-widget-tile-2 hover:text-widget-ink",
        "disabled:opacity-50",
        className,
      )}
    />
  );
}

/**
 * A tile that has a sentence to say instead of data: the source is still
 * loading, is not allowed, failed or is empty. Drawn as a widget so the board never shows a hole.
 */
export function Notice({
  size = "medium",
  label,
  title,
  children,
  action,
  accent,
}: {
  size?: WidgetSize;
  label: string;
  title: string;
  children?: ReactNode;
  action?: ReactNode;
  accent?: "calendar" | "blue";
}) {
  return (
    <WidgetFrame size={size} label={label} accent={accent}>
      <div className="flex h-full flex-col justify-center gap-1.5 p-4">
        <div className="text-notch-w-headline leading-tight font-semibold">
          {title}
        </div>
        {children && (
          <p className="m-0 text-notch-w-title leading-snug text-widget-ink-2">
            {children}
          </p>
        )}
        {action && <div className="mt-1.5 flex gap-1.5">{action}</div>}
      </div>
    </WidgetFrame>
  );
}

/**
 * A failed Calendar or Reminders read, as a sentence: a missing
 * Automation grant has one button, the System Settings flow; anything else
 * says what happened and offers a retry.
 */
export function AgendaNotice({
  error,
  app,
  permission,
  retry,
  size,
}: {
  error: AgendaError;
  app: "Calendar" | "Reminders";
  permission: "calendar" | "reminders";
  retry: () => void;
  size?: WidgetSize;
}) {
  if (error.kind === "notPermitted") {
    return (
      <Notice
        size={size}
        label={app}
        title={`Allow Aloud to read ${app}`}
        action={
          <Pill
            onClick={() => void commands.openPermissionSettings(permission)}
          >
            Allow access
          </Pill>
        }
      >
        {permission === "calendar"
          ? "macOS asks once. Aloud only reads your events; it never adds or changes one."
          : "macOS asks once. Aloud reads what is due, and ticking one off is the only change it makes."}{" "}
        Come back here after answering.
      </Notice>
    );
  }
  return (
    <Notice
      size={size}
      label={app}
      title={`Could not read ${app}`}
      action={<Pill onClick={retry}>Try again</Pill>}
    >
      {error.message}
    </Notice>
  );
}

/** A refresh that failed after one succeeded, under the board: the data stays, the reason shows. */
export function Aside({ children }: { children: ReactNode }) {
  return (
    <p className="m-0 mt-2 text-center text-notch-w-meta text-notch-danger">
      {children}
    </p>
  );
}
