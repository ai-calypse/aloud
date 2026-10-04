import { useEffect, useRef } from "react";

// Muted looping clip that only plays while on screen, and not at all for people
// who prefer reduced motion (they get the poster frame).
export default function MemeVideo({ src, poster, alt, className }: { src: string; poster: string; alt: string; className?: string }) {
  const ref = useRef<HTMLVideoElement>(null);
  useEffect(() => {
    const v = ref.current;
    if (!v || matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const io = new IntersectionObserver(([e]) => (e.isIntersecting ? v.play().catch(() => {}) : v.pause()), { threshold: 0.25 });
    io.observe(v);
    return () => io.disconnect();
  }, []);
  return <video ref={ref} src={src} poster={poster} aria-label={alt} muted loop playsInline preload="none" className={className} />;
}
