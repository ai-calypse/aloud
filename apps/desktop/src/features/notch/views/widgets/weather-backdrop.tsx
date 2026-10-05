import { useId } from "react";
import type { Backdrop } from "./weather-surface";

/**
 * Where the stars sit, in percent of the card. All in the gap between the
 * temperature and the summary: a star behind a letter reads as a speck on the
 * screen, not as a sky.
 */
const STARS: [x: number, y: number, size: number, opacity: number][] = [
  [44, 6, 2.5, 0.8], [52, 18, 2, 0.55], [58, 8, 3, 0.9], [63, 27, 2, 0.5],
  [49, 31, 2.5, 0.65], [68, 15, 2, 0.6], [41, 22, 2, 0.45], [60, 36, 2.5, 0.55],
];

/**
 * The weather behind a card's words: falling rain, drifting snow, slow cloud,
 * fog, stars. Decorative only (`aria-hidden`); the words and glyph carry the
 * weather. Every layer moves by `transform` alone, so the compositor owns the
 * frames and React renders this once. The loops stop under reduced motion
 * (motion.css), leaving a still picture of the same weather.
 *
 * The picture fades out down the card (`.wx` mask) so the five-day list stays
 * on a plain ground.
 */
export function WeatherBackdrop({ kind }: { kind: Backdrop }) {
  const id = useId();
  return (
    <div aria-hidden className="wx pointer-events-none absolute inset-0 overflow-hidden text-widget-ink">
      {kind === "stars" &&
        STARS.map(([x, y, size, o]) => (
          <i
            key={`${x}-${y}`}
            className="absolute rounded-full bg-widget-ink"
            style={{ left: `${x}%`, top: `${y}%`, width: size, height: size, opacity: o }}
          />
        ))}
      {(kind === "rain" || kind === "storm") && (
        <div className={`wx-fall wx-fall--${kind}`}>
          <svg width="100%" height="100%">
            <defs>
              <pattern id={id} width="44" height="44" patternUnits="userSpaceOnUse">
                <path d="M9 3 5 17M31 24 27 38M20 12 17 22M40 34 37 42" stroke="currentColor" strokeLinecap="round" strokeWidth="1.2" />
              </pattern>
            </defs>
            <rect width="100%" height="100%" fill={`url(#${id})`} opacity={kind === "storm" ? 0.3 : 0.22} />
          </svg>
        </div>
      )}
      {kind === "snow" && (
        <div className="wx-fall wx-fall--snow">
          <svg width="100%" height="100%">
            <defs>
              <pattern id={id} width="64" height="64" patternUnits="userSpaceOnUse">
                <circle cx="9" cy="10" r="1.7" fill="currentColor" />
                <circle cx="38" cy="26" r="1.2" fill="currentColor" />
                <circle cx="55" cy="6" r="1" fill="currentColor" />
                <circle cx="22" cy="48" r="1.4" fill="currentColor" />
                <circle cx="50" cy="55" r="1.8" fill="currentColor" />
              </pattern>
            </defs>
            <rect width="100%" height="100%" fill={`url(#${id})`} opacity={0.32} />
          </svg>
        </div>
      )}
      {kind === "clouds" && (
        <>
          <div className="wx-puff wx-puff--a" />
          <div className="wx-puff wx-puff--b" />
        </>
      )}
      {kind === "fog" && (
        <>
          <div className="wx-band wx-band--a" />
          <div className="wx-band wx-band--b" />
          <div className="wx-band wx-band--c" />
        </>
      )}
    </div>
  );
}
