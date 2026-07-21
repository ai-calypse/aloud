import React, { useEffect, useRef, useState, type ReactNode } from "react";
import ReactDOM from "react-dom/client";
import { events, type OverlayStyle, type Phase } from "./lib/bindings";
import { notchPath } from "./lib/notch-shape";
import { createSpring } from "./lib/spring";
import "./styles/overlay.css";

const LABEL: Record<Phase, string> = {
  idle: "",
  recording: "Listening…",
  transcribing: "Transcribing…",
  done: "Pasted",
  noSpeech: "No speech detected",
  // Replaced by the real reason as soon as Rust sends one; this is only what
  // shows if an error somehow arrives without a message.
  error: "Failed",
};

/** A long error still has to fit one line on a small pill. */
const ERROR_CHARS = 90;

/**
 * Bars in the listening wave.
 *
 * Five, as the Swift app used. The count and the per-bar phase step together
 * set how much of a wave fits across the row (see the delay in overlay.css) —
 * change this and that has to move with it.
 */
const BAR_COUNT = 5;

/**
 * Per-bar height ceiling: a raised cosine, tallest in the middle.
 *
 * Without it every bar reaches the same height and the row reads as a ribbon
 * rather than a waveform. Computed rather than hand-tuned so the shape holds
 * if BAR_COUNT changes.
 *
 * The floor is 0.6, not 0, because this multiplies through the wave's trough
 * and the silent-level scale as well: a shallower envelope crushed the outer
 * bars to a 1px dash at rest, which read as a broken row of dots rather than
 * as short bars.
 */
const BAR_PEAKS = Array.from({ length: BAR_COUNT }, (_, i) => {
  const position = i / (BAR_COUNT - 1); // 0…1 across the row
  return 0.6 + 0.4 * Math.sin(Math.PI * position);
});

/**
 * The drop-down panel's motion, ported from the reference implementation's
 * `balanced` preset.
 *
 * One damping ratio for everything and two responses: the close is deliberately
 * slower than the open. That asymmetry is not a detail — an equal open and
 * close reads as a box toggling, where a lingering close reads as something
 * with mass settling back. Damping below 1 lets it overshoot very slightly on
 * arrival, which is the "landing" part of the feel.
 */
const PANEL = { damping: 0.77, open: 0.45, close: 0.55 };

/**
 * How the corner radii open up as the panel grows past the cutout.
 *
 * At rest the panel *is* the notch, so it wears the notch's own corners
 * (`baseRadius = height / 3`, and the top pair 4pt tighter — the reference's
 * collapsed values, ported directly). As it grows the corners open with it,
 * which is the same thing the reference does by handing its expanded states a
 * much larger fixed pair. Derived from the *current* height rather than the
 * target, so the radii interpolate along with the box instead of jumping to
 * their final value on the first frame.
 */
const RADIUS = { topGrowth: 0.12, topMax: 16, bottomGrowth: 0.14, bottomMax: 22 };

/** Only for the `?style=pill` browser preview, where no notch is measured. */
const NOTCH_FALLBACK = { width: 200, height: 32 };

/**
 * How much wider than the cutout the panel settles, at minimum.
 *
 * The floor exists so the panel is never narrower than the hole it hangs from;
 * this is how far past it we deliberately go. Content wider than this still
 * wins, so a long error message grows the panel rather than being squeezed.
 */
const PANEL_WIDTH_FACTOR = 1.5;

/**
 * Grows the drop-down panel out of the notch and shrinks it back in.
 *
 * This is the one surface that does not use a CSS transition, for the reason
 * `lib/spring.ts` exists: its size is retargeted mid-flight (a phase change
 * lands while the previous one is still settling, three times a dictation), and
 * a transition restarts its curve from zero velocity every time it is
 * redirected. The reference gets this free from SwiftUI, whose springs are
 * interruptible by construction.
 *
 * Per frame it writes the panel's box and the shape's `d` straight onto the
 * nodes. No React state is involved — 60 renders a second is exactly what the
 * app's motion rule forbids, and the subtree would rebuild under the animation.
 */
function useNotchPanel(style: OverlayStyle, run: number, dismissing: boolean) {
  const surface = useRef<HTMLDivElement>(null);
  const shape = useRef<SVGSVGElement>(null);
  const path = useRef<SVGPathElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const driver = useRef<{
    open: () => void;
    collapse: () => void;
    remeasure: () => void;
  } | null>(null);

  // Rebuilt each time the panel appears, so every dictation starts its growth
  // from the cutout rather than from wherever the last one left the springs.
  useEffect(() => {
    const surfaceElement = surface.current;
    const shapeElement = shape.current;
    const pathElement = path.current;
    const contentElement = content.current;
    if (!surfaceElement || !shapeElement || !pathElement || !contentElement) return;
    // Nothing is on screen yet (the pill is `display: none` until the first
    // run), so there is no layout to measure and no growth to start. Measuring
    // here would set the target from a zero-width box.
    if (!contentElement.offsetWidth) return;

    let base = { width: NOTCH_FALLBACK.width, height: NOTCH_FALLBACK.height };
    let target = base;
    let baseRadius = NOTCH_FALLBACK.height / 3;
    let collapsed = false;

    // getComputedStyle forces a style recalc, so it is called when the layout
    // changes and never from inside a frame.
    const measure = () => {
      const computed = getComputedStyle(surfaceElement);
      const notchWidth =
        parseFloat(computed.getPropertyValue("--notch-w")) || NOTCH_FALLBACK.width;
      const notchHeight =
        parseFloat(computed.getPropertyValue("--notch-h")) || NOTCH_FALLBACK.height;

      // The reference's `baseRadius`: the cutout's own rounding, which every
      // other radius here is measured from.
      baseRadius = notchHeight / 3;
      base = { width: notchWidth, height: notchHeight };
      const settledHeight = notchHeight + contentElement.offsetHeight;
      target = {
        // The body sits between the two concave shoulders, so the panel has to
        // be wider than its content by exactly that much. Uses the *settled*
        // top radius, since that is the width the panel comes to rest at.
        //
        // Never narrower than the cutout: a short status word ("Pasted") wants
        // less width than the notch, and a panel narrower than the hole it
        // hangs from leaves the physical camera bump sticking out either side.
        // Every content size in the reference is likewise `baseWidth + n`.
        width: Math.max(
          notchWidth * PANEL_WIDTH_FACTOR,
          contentElement.offsetWidth + topRadiusAt(settledHeight) * 2,
        ),
        height: settledHeight,
      };
    };

    const topRadiusAt = (height: number) =>
      Math.min(
        RADIUS.topMax,
        Math.max(0, baseRadius - 4) + Math.max(0, height - base.height) * RADIUS.topGrowth,
      );

    const size = { width: 0, height: 0 };
    const draw = () => {
      const grown = Math.max(0, size.height - base.height);
      const bottomRadius = Math.min(
        RADIUS.bottomMax,
        baseRadius + grown * RADIUS.bottomGrowth,
      );

      surfaceElement.style.width = `${size.width}px`;
      surfaceElement.style.height = `${size.height}px`;
      shapeElement.setAttribute("viewBox", `0 0 ${size.width} ${size.height}`);
      pathElement.setAttribute(
        "d",
        notchPath(size.width, size.height, topRadiusAt(size.height), bottomRadius),
      );
    };

    const width = createSpring(
      (value) => {
        size.width = value;
        draw();
      },
      { damping: PANEL.damping, response: PANEL.open },
    );
    const height = createSpring(
      (value) => {
        size.height = value;
        draw();
      },
      { damping: PANEL.damping, response: PANEL.open },
    );

    measure();
    size.width = base.width;
    size.height = base.height;
    width.jump(base.width);
    height.jump(base.height);
    width.to(target.width, PANEL.open);
    height.to(target.height, PANEL.open);

    driver.current = {
      open() {
        collapsed = false;
        measure();
        width.to(target.width, PANEL.open);
        height.to(target.height, PANEL.open);
      },
      collapse() {
        collapsed = true;
        width.to(base.width, PANEL.close);
        height.to(base.height, PANEL.close);
      },
      remeasure() {
        measure();
        // A label that changes while the panel is collapsing must not drag it
        // back open — the window is already on its way out.
        if (collapsed) return;
        width.to(target.width, PANEL.open);
        height.to(target.height, PANEL.open);
      },
    };

    const observer = new ResizeObserver(() => driver.current?.remeasure());
    observer.observe(contentElement);

    return () => {
      observer.disconnect();
      width.stop();
      height.stop();
      driver.current = null;
    };
  }, [style, run]);

  // Declared after the effect above so that on a rebuild the driver exists
  // before this runs — effects fire in declaration order.
  useEffect(() => {
    if (dismissing) driver.current?.collapse();
    else driver.current?.open();
  }, [dismissing]);

  return { surface, shape, path, content };
}

/**
 * Drives the surface's width from its content, so a phase change *morphs* the
 * pill instead of snapping it.
 *
 * A normal dictation swaps the label three times in a couple of seconds
 * ("Listening…" → "Transcribing…" → "Pasted"), and an auto-width capsule jumps
 * on each one. The jump is what reads as web UI rather than as one physical
 * object changing shape — resizing is the motion, so it has to be animated.
 *
 * The observed node carries the padding and never remounts, so its border-box
 * width *is* the width the surface should take. `width: max-content` on it
 * keeps it independent of the surface it is sizing, which is what stops the
 * observer feeding back into itself. CSS owns the interpolation; this only
 * ever writes a target, so the compositor keeps every frame.
 *
 * Zero widths are dropped: the branch is `display: none` when idle, and
 * animating out of 0 would collapse the capsule to a sliver on the way in.
 */
function useMorphWidth(style: OverlayStyle) {
  const surface = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);

  // Keyed on `style` because each style renders its own branch: the refs point
  // at different nodes after Rust pushes a new one, and an observer left on the
  // old node would size a surface that no longer exists.
  useEffect(() => {
    const surfaceElement = surface.current;
    const contentElement = content.current;
    if (!surfaceElement || !contentElement) return;

    const observer = new ResizeObserver((entries) => {
      const width = entries[0]?.borderBoxSize?.[0]?.inlineSize;
      if (width) surfaceElement.style.setProperty("--morph-w", `${width}px`);
    });
    observer.observe(contentElement);
    return () => observer.disconnect();
  }, [style]);

  return { surface, content };
}

/**
 * The recording pill. Rust drives the window's visibility, so this only ever
 * renders the current phase.
 */
function Overlay() {
  // Dev/QA: `?phase=recording` renders the pill statically in a browser so it
  // can be seen and screenshotted without holding the dictate hotkey. In the
  // real app there is no query string, so this is idle until Rust drives it.
  const preview = new URLSearchParams(location.search).get("phase") as Phase | null;
  const [phase, setPhase] = useState<Phase>(preview ?? "idle");
  // Rust picks the style and pushes it on every show — including the swap back
  // to the pill for an error, which no wing is wide enough to show.
  const [style, setStyle] = useState<OverlayStyle>(
    (new URLSearchParams(location.search).get("style") as OverlayStyle) ??
      "liquidGlass",
  );
  const [error, setError] = useState<string | null>(null);
  const [download, setDownload] = useState<number | null>(null);
  // Rust tells us the window is about to go away, so the panel can collapse
  // back into the notch while there is still a surface to draw it on.
  const [dismissing, setDismissing] = useState(false);
  // Bumped once per dictation. The panel keys its springs off this so each run
  // starts from the cutout; the phase alone cannot say "a new run began",
  // because a run no longer ends by returning to `idle`.
  const [run, setRun] = useState(0);
  // Dev/QA, alongside `?phase=`: `?dismiss=800` plays the exit 800ms in. The
  // exit cannot be reached any other way outside the app — it is driven by an
  // event Rust emits — and it is the one path here that races (a dictation
  // starting inside the exit window), so it needs to be reachable to look at.
  const dismissAfter = Number(new URLSearchParams(location.search).get("dismiss"));
  useEffect(() => {
    if (!dismissAfter) return;
    const timer = setTimeout(() => setDismissing(true), dismissAfter);
    return () => clearTimeout(timer);
  }, [dismissAfter]);
  // Sizes the glass capsule to its own content. The wings are two fixed tabs
  // welded to a measured cutout, so they are deliberately not morphed.
  const { surface, content } = useMorphWidth(style);
  const panel = useNotchPanel(style, run, dismissing);
  // The root, not the bar row: --level inherits, so the wings style can draw
  // two waves off one write.
  const root = useRef<HTMLDivElement>(null);

  // Levels land every 50ms. Writing --level straight onto the node keeps them
  // out of React entirely: at 20 renders a second the pill would rebuild its
  // subtree while the compositor is mid-wave, which is exactly the stutter the
  // CSS-owned animation exists to avoid. Null (no signal yet) reads as silence.
  useEffect(() => {
    const unlisten = events.audioLevel.listen((e) => {
      const level = Math.min(1, Math.max(0, e.payload ?? 0));
      root.current?.style.setProperty("--level", String(level));
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  useEffect(() => {
    const unlistenPhase = events.dictationPhase.listen((e) => {
      // `idle` means the run is over, not that the pill should vanish: the
      // window stays up for the length of the exit, and something has to be on
      // it to animate. Hold the last real phase and start collapsing instead.
      //
      // This is what made a no-speech run cut off dead while a pasted one
      // animated. Pasting ends on `done`, which lingers and then dismisses with
      // the phase unchanged; silence, a spoken ask and a cancel all end on
      // `idle`, which used to blank the surface in the same frame the collapse
      // was asked to start.
      if (e.payload === "idle") {
        setDismissing(true);
        return;
      }

      setPhase(e.payload);
      // A phase arriving means the window is staying up, which cancels an exit
      // in flight — Rust has already bumped its own generation to match.
      setDismissing(false);
      // A new run clears the last run’s message.
      if (e.payload === "recording") {
        setError(null);
        // Rebuilds the panel so it grows out of the notch again rather than
        // resuming from wherever the last run's springs were left.
        setRun((previous) => previous + 1);
      }
    });
    const unlistenDismiss = events.overlayDismiss.listen(() => setDismissing(true));
    const unlistenError = events.overlayError.listen((e) => setError(e.payload));
    const unlistenStyle = events.overlayStyle.listen((e) => {
      setStyle(e.payload.style);
      // The exact notch cutout, measured natively, so the wings weld to it
      // rather than straddle a fixed guess. Written as vars the CSS reads;
      // zero for a pill, where the wave falls back to its own gap/height.
      root.current?.style.setProperty("--notch-w", `${e.payload.notchWidth ?? 0}px`);
      root.current?.style.setProperty(
        "--notch-h",
        `${e.payload.notchHeight ?? 0}px`,
      );
    });
    // First use of the local engine fetches ~150MB; without this the pill sits
    // on "Transcribing…" for minutes and the app looks hung.
    const unlistenDownload = events.modelDownload.listen((e) =>
      setDownload(e.payload === null || e.payload >= 1 ? null : e.payload),
    );
    return () => {
      unlistenPhase.then((off) => off());
      unlistenDismiss.then((off) => off());
      unlistenError.then((off) => off());
      unlistenStyle.then((off) => off());
      unlistenDownload.then((off) => off());
    };
  }, []);

  // The live transcript is no longer shown here — it has its own panel. The
  // pill is back to a status word plus the wave.
  const label =
    phase === "error" && error
      ? truncate(error, ERROR_CHARS)
      : download !== null
        ? `Downloading model… ${Math.round(download * 100)}%`
        : LABEL[phase];

  return (
    // The exit is a state of the whole surface, not of one style: all three
    // now have the window held open for them, so all three have to use it.
    <div className="overlay-root" ref={root} data-exiting={dismissing || undefined}>
    {/* Refraction filter for the glass pill: turbulence drives a displacement
        map that warps the glass highlight layer. WebKit renders this on an
        element's own content (unlike backdrop-filter: url(), which Safari does
        not support), so it works in the Tauri webview. */}
    {style === "liquidGlass" && (
      <svg width="0" height="0" aria-hidden style={{ position: "absolute" }}>
        <filter id="pill-refraction" x="-20%" y="-20%" width="140%" height="140%">
          <feTurbulence
            type="fractalNoise"
            baseFrequency="0.012 0.02"
            numOctaves={2}
            seed={7}
            result="noise"
          />
          <feGaussianBlur in="noise" stdDeviation="1.2" result="softNoise" />
          <feDisplacementMap
            in="SourceGraphic"
            in2="softNoise"
            scale={22}
            xChannelSelector="R"
            yChannelSelector="G"
          />
        </filter>
      </svg>
    )}
    {style === "wings" ? (
      // voice-dict's minimalist overlay: one continuous black bar welded to the
      // notch — the waveform on the left, a recording light on the right, the
      // measured cutout between them. Only ever shown on a screen that has a
      // notch; Rust falls back to a pill otherwise (see overlay.rs).
      <div className={`wings wings--${phase}`}>
        <span className="wing wing--left">
          <PhaseSwap phase={phase}>
            <Indicator phase={phase} />
          </PhaseSwap>
        </span>
        {/* The notch itself: solid black, exactly the cutout's width, so the
            bar is continuous across it and the camera notch disappears into
            the strip — the whole point of the blend. */}
        <span className="wing-notch" aria-hidden />
        <span className="wing wing--right">
          {phase === "recording" && (
            <span className="dot dot--recording" aria-hidden />
          )}
        </span>
      </div>
    ) : style === "pill" ? (
      // voice-dict's drop-down pill: a solid-black panel welded to the top,
      // rounded only along the bottom, with the wave and status sitting just
      // below the notch (the __notch spacer is the measured cutout height).
      <div className={`drop-pill drop-pill--${phase}`} ref={panel.surface}>
        {/* The panel's whole silhouette. A path and not a border-radius,
            because the two top corners are concave — see lib/notch-shape.ts. */}
        <svg
          className="drop-pill__shape"
          ref={panel.shape}
          preserveAspectRatio="none"
          aria-hidden
        >
          <path ref={panel.path} fill="var(--notch-fill)" />
        </svg>
        <span className="drop-pill__notch" aria-hidden />
        <div className="drop-pill__body" ref={panel.content}>
          <PhaseSwap phase={phase}>
            <Indicator phase={phase} />
            <span className="label" title={error ?? undefined}>
              {label}
            </span>
          </PhaseSwap>
        </div>
      </div>
    ) : (
      // The liquid-glass pill: a refractive capsule below the menu bar, with
      // the warped sheen (below) riding over the backdrop blur.
      <div className={`pill pill--${phase}`} ref={surface}>
        <span className="pill__refraction" aria-hidden />
        {/* Carries the capsule's padding so its border-box width is exactly the
            width the capsule should morph to. */}
        <div className="pill__morph" ref={content}>
          <PhaseSwap phase={phase}>
            <Indicator phase={phase} />
            <span className="label" title={error ?? undefined}>
              {label}
            </span>
          </PhaseSwap>
        </div>
      </div>
    )}
    </div>
  );
}

/**
 * Cross-fades its contents whenever the phase changes.
 *
 * The `key` is the whole mechanism: React remounts on a new phase, so the CSS
 * entrance in overlay.css runs again without the usual reflow-to-restart-a-
 * keyframe dance. Cheap here because the subtree is a dot (or five bars) and a
 * word — and the bars only exist in one phase, so nothing mid-wave is thrown
 * away by the remount.
 */
function PhaseSwap({ phase, children }: { phase: Phase; children: ReactNode }) {
  return (
    <span key={phase} className="phase-swap">
      {children}
    </span>
  );
}

/** The wave while recording, a status dot otherwise. Shared by both styles. */
function Indicator({ phase }: { phase: Phase }) {
  if (phase !== "recording") {
    return <span className={`dot dot--${phase}`} aria-hidden />;
  }
  // A continuous wave the voice scales rather than drives. CSS owns the
  // motion; these two custom properties only give each bar its place in the
  // row and its height ceiling.
  return (
    <span className="bars" aria-hidden>
      {BAR_PEAKS.map((peak, i) => (
        <i key={i} style={{ "--i": i, "--peak": peak } as React.CSSProperties} />
      ))}
    </span>
  );
}

/** Cuts on a char boundary so a multi-byte character is never split. */
function truncate(text: string, max: number): string {
  const flat = text.replace(/\s+/g, " ").trim();
  return [...flat].length <= max ? flat : `${[...flat].slice(0, max).join("")}…`;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
