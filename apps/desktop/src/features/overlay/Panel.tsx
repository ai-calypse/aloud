import React, { useState, type ReactNode } from "react";
import {
  CheckIcon,
  CopyIcon,
  PlusIcon,
  XIcon,
} from "lucide-react";

/**
 * The chrome an agent run draws inside the notch panel: what was heard,
 * a rule, the scrolling body, and a footer.
 */
export function Panel({
  title,
  bodyRef,
  /** Omit to hide the copy button — there is nothing worth copying yet. */
  copyText,
  onClose,
  onAddTab,
  footer,
  children,
}: {
  title: string;
  bodyRef?: React.RefObject<HTMLDivElement | null>;
  copyText?: string;
  onClose: () => void;
  /** Omitted at MAX_TABS — the button goes away rather than sitting dead. */
  onAddTab?: () => void;
  /** Sits left of new/copy/close. */
  footer?: ReactNode;
  children: ReactNode;
}) {
  const [copied, setCopied] = useState(false);

  return (
    <div className="ask">
      <p className="ask__question">{title}</p>
      <div className="ask__answer" ref={bodyRef}>
        {children}
      </div>
      <div className="ask__footer">
        {footer}
        {/* The second way to open a tab. The strip's own `+` is the discoverable
            one; this sits in the row the eye already goes to for copy/close.
            Absent, not disabled, at the cap — matching the strip's. */}
        {onAddTab && (
          <button
            className="ask__button"
            aria-label="New tab"
            title="New tab"
            onClick={onAddTab}
          >
            <PlusIcon size={15} strokeWidth={2.5} />
          </button>
        )}
        {copyText !== undefined && (
          <button
            className="ask__button"
            aria-label={copied ? "Copied" : "Copy"}
            onClick={() =>
              void navigator.clipboard.writeText(copyText).then(() => setCopied(true))
            }
          >
            {copied ? <CheckIcon size={14} /> : <CopyIcon size={14} />}
          </button>
        )}
        <button className="ask__button" aria-label="Close" onClick={onClose}>
          <XIcon size={14} />
        </button>
      </div>
    </div>
  );
}
