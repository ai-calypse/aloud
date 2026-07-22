"use client";

/**
 * Trimmed from the upstream AI Elements `message` (448 lines: branch
 * navigation, attachments, an actions toolbar) to the one export this app
 * uses — it renders one answer at a time, with no thread to branch. Dropping
 * the rest also removed the only importers of ui/tooltip and ui/button-group.
 *
 * Re-pull from the registry if a real conversation view ever lands.
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
