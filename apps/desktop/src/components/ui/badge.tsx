import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { Slot } from "radix-ui"

import { cn } from "@/lib/utils"

/** A chip on the notch: a bordered well, quieter than any button beside it. */
const NOTCH_CHIP =
  "h-6 rounded-notch-chip border-notch-edge bg-notch-well px-2 py-0 font-notch text-notch-meta font-medium text-notch-ink-2"

const badgeVariants = cva(
  "inline-flex w-fit shrink-0 items-center justify-center gap-1 overflow-hidden rounded-full border border-transparent px-2 py-0.5 text-xs font-medium whitespace-nowrap transition-[color,box-shadow] focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 aria-invalid:border-destructive aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 [&>svg]:pointer-events-none [&>svg]:size-3",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground [a&]:hover:bg-primary/90",
        secondary:
          "bg-secondary text-secondary-foreground [a&]:hover:bg-secondary/90",
        destructive:
          "bg-destructive text-white focus-visible:ring-destructive/20 dark:bg-destructive/60 dark:focus-visible:ring-destructive/40 [a&]:hover:bg-destructive/90",
        outline:
          "border-border text-foreground [a&]:hover:bg-accent [a&]:hover:text-accent-foreground",
        ghost: "[a&]:hover:bg-accent [a&]:hover:text-accent-foreground",
        link: "text-primary underline-offset-4 [a&]:hover:underline",

        /* --- The notch panel. A status chip always carries its word; the
           colour only confirms it ("Done" in green, never a green dot). */
        notch: NOTCH_CHIP,
        "notch-ok": `${NOTCH_CHIP} text-notch-ok`,
        "notch-warn": `${NOTCH_CHIP} text-notch-warn`,
        "notch-danger": `${NOTCH_CHIP} text-notch-danger`,
        /** How many need you, on a section tab: Linear's count, inverted. */
        "notch-count":
          "h-4 min-w-4 rounded-notch-key border-0 bg-notch-primary px-1 font-notch-mono text-notch-micro font-semibold text-notch-primary-ink tabular-nums",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
)

function Badge({
  className,
  variant = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"span"> &
  VariantProps<typeof badgeVariants> & { asChild?: boolean }) {
  const Comp = asChild ? Slot.Root : "span"

  return (
    <Comp
      data-slot="badge"
      data-variant={variant}
      className={cn(badgeVariants({ variant }), className)}
      {...props}
    />
  )
}

export { Badge, badgeVariants }
