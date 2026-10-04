import { useEffect, useRef, type CSSProperties } from "react";

// A small tilted picture: rounded corners and a soft shadow, no window chrome.
// It is visible by default; the first time it scrolls into view it pops in from a
// slight scale and tilt (WAAPI, so a hidden tab or a slow observer never leaves it blank).
export default function Sticker({ rotate = 0, className = "", children }: { rotate?: number; className?: string; children: React.ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el || matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const io = new IntersectionObserver(
      ([e]) => {
        if (!e.isIntersecting) return;
        io.disconnect();
        el.animate(
          [
            { opacity: 0, transform: `translateY(12px) scale(0.92) rotate(${rotate - 8}deg)` },
            { opacity: 1, transform: `translateY(0) scale(1) rotate(${rotate}deg)` },
          ],
          { duration: 460, easing: "cubic-bezier(0.23, 1, 0.32, 1)" },
        );
      },
      { threshold: 0.35 },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [rotate]);
  return (
    <div ref={ref} className={`sticker overflow-hidden rounded-[12px] shadow-window [&>*]:block [&>*]:w-full ${className}`} style={{ "--r": `${rotate}deg` } as CSSProperties}>
      {children}
    </div>
  );
}
