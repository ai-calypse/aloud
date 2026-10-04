import {
  AppWindowIcon,
  ArrowDownToLineIcon,
  BatteryIcon,
  BellIcon,
  BotIcon,
  CameraIcon,
  CheckIcon,
  MicIcon,
  MusicIcon,
  TimerIcon,
  type LucideIcon,
} from "lucide-react";
import { cn } from "@/lib/utils";

/**
 * The glyphs an island or row may name instead of sending an image. Rust
 * sends a name ("agents") when there is no app to take an icon from; add one
 * here when a feature needs it, rather than an emoji in the title.
 */
export const GLYPHS: Record<string, LucideIcon> = {
  agents: BotIcon,
  alert: BellIcon,
  battery: BatteryIcon,
  camera: CameraIcon,
  done: CheckIcon,
  download: ArrowDownToLineIcon,
  mic: MicIcon,
  music: MusicIcon,
  timer: TimerIcon,
};

const SIZE = {
  sm: "size-[18px] rounded-[5px] [&_svg]:size-3",
  md: "size-[26px] rounded-notch-control [&_svg]:size-3.5",
  lg: "size-[30px] rounded-notch-control [&_svg]:size-4",
};

/**
 * An app's own icon, a named glyph, or a neutral window. The images are the
 * ones Rust already makes for cards and toasts — `aloud_selection::app_icon`
 * for a Mac app, `aloud_agent::icons` for a connected service — always
 * `data:` URIs, the only images the overlay's CSP loads.
 */
export function AppIcon({
  icon,
  label,
  size = "md",
  className,
}: {
  /** A `data:` image, a key of `GLYPHS`, or nothing. */
  icon?: string | null;
  /** What it is, for assistive tech: the app or the activity. */
  label: string;
  size?: keyof typeof SIZE;
  className?: string;
}) {
  if (icon?.startsWith("data:")) {
    return (
      <img
        className={cn("shrink-0 object-contain", SIZE[size], className)}
        src={icon}
        alt={label}
        draggable={false}
      />
    );
  }
  const Glyph = (icon && GLYPHS[icon]) || AppWindowIcon;
  return (
    <span
      role="img"
      aria-label={label}
      className={cn(
        "inline-flex shrink-0 items-center justify-center bg-notch-well text-notch-ink-2",
        SIZE[size],
        className,
      )}
    >
      <Glyph strokeWidth={1.8} aria-hidden />
    </span>
  );
}
