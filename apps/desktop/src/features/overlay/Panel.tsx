import React, { useState, type ReactNode } from "react";
import { CheckIcon, CopyIcon, Maximize2Icon, XIcon } from "lucide-react";

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
  onExpand,
  footer,
  children,
}: {
  title: string;
  bodyRef?: React.RefObject<HTMLDivElement | null>;
  copyText?: string;
  onClose: () => void;
  /** Only on the compact run panel: opens the same run in Agents › Voice. */
  onExpand?: () => void;
  /** Sits left of expand/copy/close. */
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
        {onExpand && (
          <button
            className="ask__button"
            aria-label="Open in Agents"
            title="Open in Agents › Voice"
            onClick={onExpand}
          >
            <Maximize2Icon size={14} />
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
