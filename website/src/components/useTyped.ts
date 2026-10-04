import { useEffect, useRef, useState } from "react";

// Types `text` out the first time the element is on screen, then stays. Returns how many
// characters to show. Everything is shown straight away for reduced motion, and the
// caller should render the full text invisibly so the layout never shifts.
export function useTyped<T extends HTMLElement>(text: string, { speed = 42, start = 0 }: { speed?: number; start?: number } = {}) {
  const ref = useRef<T>(null);
  const [n, setN] = useState(() => (typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches ? text.length : 0));
  useEffect(() => {
    const el = ref.current;
    if (!el || matchMedia("(prefers-reduced-motion: reduce)").matches) {
      setN(text.length);
      return;
    }
    setN(0);
    let timer = 0;
    const io = new IntersectionObserver(
      ([e]) => {
        if (!e.isIntersecting) return;
        io.disconnect();
        let i = 0;
        const tick = () => {
          setN(++i);
          if (i < text.length) timer = window.setTimeout(tick, speed + Math.random() * speed * 0.6);
        };
        timer = window.setTimeout(tick, start);
      },
      { threshold: 0.6 },
    );
    io.observe(el);
    return () => {
      io.disconnect();
      clearTimeout(timer);
    };
  }, [text, speed, start]);
  return [ref, n] as const;
}
