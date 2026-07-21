import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { Slot } from "radix-ui"

import { cn } from "@/lib/utils"

const buttonVariants = cva(
  // `transition-all` was shadcn's default and it animates every property that
  // ever changes, including width and content shifts. Naming the four that
  // should move keeps a button from easing its own layout.
  //
  // The press scale is on the base, not per-variant: a control that does not
  // acknowledge the pointer on the way *down* feels dead no matter how good
  // the rest of the motion is. Variants that want a different amount still
  // override it — theirs are merged after these.
  "inline-flex shrink-0 items-center justify-center gap-2 rounded-md text-sm font-medium whitespace-nowrap outline-none transition-[color,background-color,border-color,box-shadow,transform] duration-[var(--dur-fast)] ease-[var(--ease-out)] active:scale-[0.97] focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50 disabled:active:scale-100 aria-invalid:border-destructive aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground hover:bg-primary/90",
        destructive:
          "bg-destructive text-white hover:bg-destructive/90 focus-visible:ring-destructive/20 dark:bg-destructive/60 dark:focus-visible:ring-destructive/40",
        outline:
          "border bg-background shadow-xs hover:bg-accent hover:text-accent-foreground dark:border-input dark:bg-input/30 dark:hover:bg-input/50",
        secondary:
          "bg-secondary text-secondary-foreground hover:bg-secondary/80",
        ghost:
          "hover:bg-accent hover:text-accent-foreground dark:hover:bg-accent/50",
        link: "text-primary underline-offset-4 hover:underline",

        /* --- Aloud's macOS variants ---------------------------------------
         * The app's own buttons, previously hand-rolled as raw <button> class
         * strings duplicated across MacroEditor, AppRuleEditor, SettingsView,
         * ConnectorEditor and App.tsx. Kept here so there is one button in the
         * app rather than shadcn's plus a parallel set.
         */
        /** Filled action: "Add Rule", "Add Macro", "Connect". */
        accent:
          "bg-brand text-brand-fg shadow-accent hover:bg-brand/90 active:scale-[0.97]",
        /** Text-only action inside a row: "Edit", "Add", "Preview". */
        plain: "text-brand hover:text-brand/80 active:scale-95",
        /** Its destructive twin: "Delete", "Disconnect". */
        "plain-danger": "text-danger hover:text-danger/80 active:scale-95",
        /** Filled but quiet: the Connect button in a Connectors row. */
        field: "bg-field hover:bg-brand hover:text-brand-fg",
      },
      size: {
        default: "h-9 px-4 py-2 has-[>svg]:px-3",
        xs: "h-6 gap-1 rounded-md px-2 text-xs has-[>svg]:px-1.5 [&_svg:not([class*='size-'])]:size-3",
        sm: "h-8 gap-1.5 rounded-md px-3 has-[>svg]:px-2.5",
        lg: "h-10 rounded-md px-6 has-[>svg]:px-4",
        icon: "size-9",
        "icon-xs": "size-6 rounded-md [&_svg:not([class*='size-'])]:size-3",
        "icon-sm": "size-8",
        "icon-lg": "size-10",
        /** The design's row-height control: 13px text in a 15/7 box. */
        row: "rounded-lg px-[15px] py-[7px] text-ui font-medium",
        /** Text-only actions carry no box, so they get no padding either. */
        text: "text-ui font-medium",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

function Button({
  className,
  variant = "default",
  size = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean
  }) {
  const Comp = asChild ? Slot.Root : "button"

  return (
    <Comp
      data-slot="button"
      data-variant={variant}
      data-size={size}
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  )
}

export { Button, buttonVariants }
