import type { ComponentProps } from "react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/** The notch's text field: a Well ground with an Edge border, the control shape. */
const FIELD = cn(
  "w-full rounded-notch-row border border-notch-edge bg-notch-well px-3 font-notch text-notch-body",
  "text-notch-ink outline-none placeholder:text-notch-ink-3 disabled:opacity-50",
  NOTCH_FOCUS,
);

export function TextField({ className, ...props }: ComponentProps<"input">) {
  return <input type="text" className={cn(FIELD, "h-[26px]", className)} {...props} />;
}

export function TextArea({ className, ...props }: ComponentProps<"textarea">) {
  return <textarea className={cn(FIELD, "resize-none py-2", className)} {...props} />;
}
