import { cn } from "@/lib/utils";
import type { CSSProperties, ElementType } from "react";

export type ShimmerProps = {
  children: string;
  as?: ElementType;
  className?: string;
  /** Width of the bright band, in px per character. */
  spread?: number;
};

/**
 * Text with a highlight sweeping across it, for "the agent is working".
 *
 * The sweep is a CSS keyframe (`motion.css`), not a JS animation loop: the
 * compositor owns the frames, and the global reduced-motion switch reaches it.
 * The only thing JS decides is how wide the band is, which depends on the
 * text length and so cannot live in the stylesheet.
 */
export function Shimmer({
  children,
  as: Component = "p",
  className,
  spread = 2,
}: ShimmerProps) {
  return (
    <Component
      className={cn(
        "motion-shimmer relative inline-block bg-[length:250%_100%,auto] bg-clip-text text-transparent",
        "[--bg:linear-gradient(90deg,#0000_calc(50%-var(--spread)),var(--color-background),#0000_calc(50%+var(--spread)))] [background-repeat:no-repeat,padding-box]",
        className,
      )}
      style={
        {
          "--spread": `${(children?.length ?? 0) * spread}px`,
          backgroundImage:
            "var(--bg), linear-gradient(var(--color-muted-foreground), var(--color-muted-foreground))",
        } as CSSProperties
      }
    >
      {children}
    </Component>
  );
}
