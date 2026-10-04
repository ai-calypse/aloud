// Ports of the aloud design system's components (bundle.js there is a global
// script on React 18; these are the same markup and class names, typed).
import { useId, type CSSProperties, type ReactNode } from "react";
import SmartLink from "./SmartLink";
import { useTyped } from "./useTyped";

const cx = (...a: (string | false | undefined)[]) => a.filter(Boolean).join(" ");

const APPLE =
  "M12.152 6.896c-.948 0-2.415-1.078-3.96-1.04-2.04.027-3.91 1.183-4.961 3.014-2.117 3.675-.546 9.103 1.519 12.09 1.013 1.454 2.208 3.09 3.792 3.039 1.52-.065 2.09-.987 3.935-.987 1.831 0 2.35.987 3.96.948 1.637-.026 2.676-1.48 3.676-2.948 1.156-1.688 1.636-3.325 1.662-3.415-.039-.013-3.182-1.221-3.22-4.857-.026-3.04 2.48-4.494 2.597-4.559-1.429-2.09-3.623-2.324-4.39-2.376-2-.156-3.675 1.09-4.61 1.09zM15.53 3.83c.843-1.012 1.4-2.427 1.245-3.83-1.207.052-2.662.805-3.532 1.818-.78.896-1.454 2.338-1.273 3.714 1.338.104 2.715-.688 3.559-1.701";

function AppleIcon() {
  return (
    <svg className="al-ico" viewBox="0 0 24 24" aria-hidden="true">
      <path d={APPLE} />
    </svg>
  );
}

function WindowsIcon() {
  return (
    <svg className="al-ico" viewBox="0 0 24 24" aria-hidden="true">
      <rect x="1" y="1" width="10.4" height="10.4" />
      <rect x="12.6" y="1" width="10.4" height="10.4" />
      <rect x="1" y="12.6" width="10.4" height="10.4" />
      <rect x="12.6" y="12.6" width="10.4" height="10.4" />
    </svg>
  );
}

type GlossyButtonProps = {
  tone?: "blue" | "white";
  size?: "md" | "sm";
  href?: string;
  icon?: "apple" | "windows";
  submit?: boolean;
  disabled?: boolean;
  onClick?: () => void;
  event?: string;
  children: ReactNode;
};

export function GlossyButton({ tone = "blue", size = "md", href, icon, submit, disabled, onClick, event, children }: GlossyButtonProps) {
  const className = cx("al-gbtn", `al-gbtn--${tone}`, size === "sm" && "al-gbtn--sm");
  const inner = (
    <>
      {icon === "apple" && <AppleIcon />}
      {icon === "windows" && <WindowsIcon />}
      <span>{children}</span>
    </>
  );
  return href ? (
    <SmartLink className={className} href={href} onClick={onClick} data-event={event}>
      {inner}
    </SmartLink>
  ) : (
    <button className={className} type={submit ? "submit" : "button"} disabled={disabled} onClick={onClick}>
      {inner}
    </button>
  );
}

export function MacWindow({ title, caption, style, className, children }: { title?: string; caption?: string; style?: CSSProperties; className?: string; children?: ReactNode }) {
  return (
    <figure className={cx("al-mw", className)} style={style}>
      <div className="al-mw-win">
        <div className="al-mw-bar">
          <span className="al-dots" aria-hidden="true">
            <i />
            <i />
            <i />
          </span>
          <span className="al-mw-title">{title}</span>
        </div>
        <div className="al-mw-body">{children}</div>
      </div>
      {caption && <figcaption className="al-mw-cap">{caption}</figcaption>}
    </figure>
  );
}

export function Notch({ text }: { text: string }) {
  return (
    <div className="al-notch" role="img" aria-label={text}>
      <span className="al-wave" aria-hidden="true">
        <i />
        <i />
        <i />
        <i />
        <i />
      </span>
      <span className="al-typed">{text}</span>
    </div>
  );
}

export const PillLabel = ({ children }: { children: ReactNode }) => <span className="al-label">{children}</span>;
// The bubble types its text the first time it is seen. The full text is laid out
// invisibly underneath so the bubble keeps its final size from the start.
export function Blip({ children, className = "" }: { children: string; className?: string }) {
  const [ref, n] = useTyped<HTMLSpanElement>(children, { start: 250 });
  const typing = n < children.length;
  return (
    <span ref={ref} className="al-blip" aria-label={children}>
      <span className={`grid ${className}`} aria-hidden="true">
        <span className="invisible col-start-1 row-start-1">{children}</span>
        <span className={`col-start-1 row-start-1 ${typing ? "typing-caret" : ""}`}>{children.slice(0, n)}</span>
      </span>
    </span>
  );
}

export function GlassCard({ chips, children }: { chips?: string[]; children?: ReactNode }) {
  return (
    <div className="al-glass">
      {children && <p>{children}</p>}
      {chips && (
        <ul className="al-chips">
          {chips.map((c) => (
            <li key={c}>{c}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

export const DitherEdge = ({ position = "bottom" }: { position?: "bottom" | "top" }) => (
  <div className={cx("al-edge", `al-edge--${position}`)} aria-hidden="true" />
);

const GLYPHS: Record<string, string[]> = {
  a: [".....", ".....", ".###.", "....#", ".####", "#...#", ".####"],
  l: ["##.", ".#.", ".#.", ".#.", ".#.", ".#.", ".##"],
  o: [".....", ".....", ".###.", "#...#", "#...#", "#...#", ".###."],
  u: [".....", ".....", "#...#", "#...#", "#...#", "#..##", ".##.#"],
  d: ["....#", "....#", ".####", "#...#", "#...#", "#...#", ".####"],
};

export function PixelWordmark({ word = "aloud", width = 240 }: { word?: string; width?: number }) {
  const id = useId();
  const cells: [number, number][] = [];
  let x0 = 0;
  for (const ch of word) {
    const g = GLYPHS[ch];
    if (!g) continue;
    g.forEach((row, y) => [...row].forEach((c, x) => c === "#" && cells.push([x0 + x, y])));
    x0 += g[0].length + 1;
  }
  const w = Math.max(x0 - 1, 1);
  return (
    <svg className="al-wm" viewBox={`0 0 ${w} 7`} width={width} style={{ maxWidth: "100%" }} role="img" aria-label={word}>
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="var(--blue-300)" />
          <stop offset="1" stopColor="var(--blue-500)" />
        </linearGradient>
      </defs>
      {cells.map(([x, y]) => (
        <rect key={`${x}-${y}`} x={x + 0.06} y={y + 0.06} width={0.88} height={0.88} rx={0.14} fill={`url(#${id})`} />
      ))}
    </svg>
  );
}

const SPRITES = {
  heart: [".##.##.", "#######", "#######", ".#####.", "..###..", "...#..."],
  star: ["...#...", "...#...", "#######", ".#####.", "..###..", ".##.##.", "##...##"],
  folder: [".####......", "###########", "###########", "###########", "###########", "###########", "###########", "###########"],
  cursor: ["#........", "##.......", "###......", "####.....", "#####....", "######...", "#######..", "########.", "#####....", "##.##....", "#..##....", "...##....", "....##..."],
  check: [".......##", "......##.", "#....##..", "##..##...", ".####....", "..##....."],
  mic: ["...###...", "..#####..", "..#####..", "..#####..", "#.#####.#", "#.#####.#", "##.###.##", ".#######.", "...###...", "...###...", "..#####.."],
  arrow: ["#....", "##...", "###..", "####.", "###..", "##...", "#...."],
  lock: ["..#####..", ".#.....#.", ".#.....#.", "#########", "#########", "####.####", "####.####", "#########", "#########"],
  doc: ["######...", "#....##..", "#....#.#.", "#....####", "#.......#", "#.#####.#", "#.......#", "#.###...#", "#.......#", "#.####..#", "#########"],
  zero: ["#####", "#...#", "#...#", "#...#", "#...#", "#...#", "#####"],
};

export function PixelSprite({ name, size = 4, color }: { name: keyof typeof SPRITES; size?: number; color?: string }) {
  const rows = SPRITES[name];
  const rects: ReactNode[] = [];
  rows.forEach((row, y) => {
    let x = 0;
    while (x < row.length) {
      if (row[x] !== "#") {
        x++;
        continue;
      }
      const s = x;
      while (x < row.length && row[x] === "#") x++;
      rects.push(<rect key={`${y}-${s}`} x={s} y={y} width={x - s} height={1} />);
    }
  });
  return (
    <svg
      className="al-px"
      viewBox={`0 0 ${rows[0].length} ${rows.length}`}
      width={rows[0].length * size}
      height={rows.length * size}
      shapeRendering="crispEdges"
      aria-hidden="true"
      style={{ color: color ?? "var(--blue-500)" }}
    >
      {rects}
    </svg>
  );
}

export function ApprovalCard({ agent = "claude code", command = "npm test" }: { agent?: string; command?: string }) {
  return (
    <div className="al-term">
      <div className="al-term-dim">{agent}</div>
      <div>
        wants to run: <b>{command}</b>
      </div>
      <div className="al-term-ask">
        <GlossyButton size="sm">allow</GlossyButton>
        <GlossyButton size="sm" tone="white">
          deny
        </GlossyButton>
      </div>
    </div>
  );
}

// The aloud mark: three bars of different heights, single ink.
export function LogoMark({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 20" aria-hidden="true" fill="currentColor">
      <rect x="1" y="12" width="4" height="8" />
      <rect x="6" y="0" width="4" height="20" />
      <rect x="11" y="8" width="4" height="12" />
    </svg>
  );
}
