"use client";

/**
 * Trimmed from the upstream AI Elements `message` component to the one export
 * this app uses.
 *
 * The original shipped 448 lines: a message row, branch navigation with its
 * own context provider, attachments, an actions toolbar. Aloud renders one
 * answer at a time in the ask and agent panels — there is no thread to branch
 * and no toolbar — so everything but `MessageResponse` was unreachable, and it
 * was the only importer of ui/tooltip and ui/button-group, which went with it.
 *
 * Re-pull from the AI Elements registry if a real conversation view ever lands.
 */

import { cn } from "@/lib/utils";
import type { ComponentProps } from "react";
import { memo } from "react";
import { Streamdown } from "streamdown";

export type MessageResponseProps = ComponentProps<typeof Streamdown>;

export const MessageResponse = memo(
  ({ className, ...props }: MessageResponseProps) => (
    <Streamdown
      className={cn(
        "size-full [&>*:first-child]:mt-0 [&>*:last-child]:mb-0",
        className
      )}
      {...props}
    />
  ),
  (prevProps, nextProps) => prevProps.children === nextProps.children
);

MessageResponse.displayName = "MessageResponse";
