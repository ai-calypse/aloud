/**
 * The liquid-glass refraction layer shared by the overlay windows (ask, agent).
 *
 * An SVG turbulence → displacement filter warps a highlight/chromatic sheen
 * (`.glass-refraction`, styled per window) so the glass surface ripples. WebKit
 * renders `feDisplacementMap` on an element's own content — unlike
 * `backdrop-filter: url()`, which Safari does not support — so this works in the
 * Tauri webview. Render it once inside the glass container; the CSS class targets
 * the second element (the div here).
 */
export function GlassRefraction() {
  return (
    <>
      <svg width="0" height="0" aria-hidden style={{ position: "absolute" }}>
        <filter
          id="glass-refraction"
          x="-20%"
          y="-20%"
          width="140%"
          height="140%"
        >
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
      <span className="glass-refraction" aria-hidden />
    </>
  );
}
