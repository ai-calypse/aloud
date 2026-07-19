import type { ReactNode, RefObject } from "react";
import { CheckIcon, CopyIcon, XIcon } from "lucide-react";
import { useState } from "react";
import {
  Card,
  CardContent,
  CardFooter,
  CardHeader,
} from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Button } from "@/components/ui/button";

/**
 * The floating panel window: a single card over the user's desktop.
 *
 * The ask and agent windows had byte-identical copies of this chrome — the
 * dragable header, the copy button with its copied-state swap, the close
 * button — differing only in which command closes the window.
 */
export function PanelShell({
  /** The prompt, shown in the header. Doubles as the window's drag handle. */
  title,
  /** Set by the autosize hook; the body scrolls past the screen-derived cap. */
  bodyRef,
  cardRef,
  /** Text the copy button puts on the clipboard. Omit to hide the button. */
  copyText,
  onClose,
  children,
  footer,
}: {
  title: string;
  bodyRef?: RefObject<HTMLDivElement | null>;
  cardRef?: RefObject<HTMLDivElement | null>;
  copyText?: string;
  onClose: () => void;
  children: ReactNode;
  /** Extra controls, placed left of copy/close. */
  footer?: ReactNode;
}) {
  const [copied, setCopied] = useState(false);

  return (
    <Card
      ref={cardRef}
      className="panel gap-3 rounded-2xl border-white/10 py-4 shadow-none"
    >
      <CardHeader className="px-4">
        <p
          className="m-0 line-clamp-2 text-ui font-medium text-muted-foreground"
          data-tauri-drag-region
        >
          {title}
        </p>
      </CardHeader>
      <Separator className="bg-white/10" />

      <CardContent
        className="panel__answer px-4 text-[15px] leading-normal"
        ref={bodyRef}
      >
        {children}
      </CardContent>

      <CardFooter className="gap-2.5 px-4">
        {footer}
        <span className="flex-1" />
        {copyText !== undefined && (
          <Button
            variant="secondary"
            size="sm"
            className="rounded-full"
            onClick={() =>
              navigator.clipboard.writeText(copyText).then(() => setCopied(true))
            }
          >
            {copied ? <CheckIcon /> : <CopyIcon />}
            {copied ? "Copied" : "Copy"}
          </Button>
        )}
        <Button
          variant="secondary"
          size="icon-sm"
          className="rounded-full"
          aria-label="Close"
          onClick={onClose}
        >
          <XIcon />
        </Button>
      </CardFooter>
    </Card>
  );
}
