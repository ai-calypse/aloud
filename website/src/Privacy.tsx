import { useEffect, useRef, useState } from "react";
import "./privacy.css";
import booth from "./assets/memes/privacy-booth.webp";
import blinds from "./assets/memes/privacy-blinds.webp";
import aiCat from "./assets/memes/ai-cat.webp";
import cat from "./assets/memes/privacy-cat.webp";
import catMac from "./assets/memes/cat-mac.mp4";
import catMacPoster from "./assets/memes/cat-mac-poster.webp";
import tv from "./assets/memes/privacy-tv.webp";
import { GlossyButton, MacWindow, PixelSprite } from "./components/ds";
import MemeVideo from "./components/MemeVideo";
import Sticker from "./components/Sticker";
import { analyticsEnabled } from "./analytics";
import { useCopy } from "./copy";
import { useMeta } from "./seo";

const reduceMotion = () => matchMedia("(prefers-reduced-motion: reduce)").matches;

function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label: string }) {
  return <button className="sw" role="switch" aria-checked={on} aria-label={label} onClick={() => onChange(!on)} />;
}

const PHRASES = ["move my 3pm to friday", "reply to maya: friday works for me.", "add dentist, tuesday at 4.", "play something calm."];

function DictationRow() {
  const c = useCopy().privacy;
  const [wifi, setWifi] = useState(true);
  const [cloud, setCloud] = useState(false);
  const [typed, setTyped] = useState("");
  useEffect(() => {
    if (reduceMotion()) return setTyped(PHRASES[0]);
    let p = 0, i = 0, id: number;
    const type = () => {
      const s = PHRASES[p];
      if (i <= s.length) {
        setTyped(s.slice(0, i++));
        id = window.setTimeout(type, 70);
      } else {
        id = window.setTimeout(() => { i = 0; p = (p + 1) % PHRASES.length; type(); }, 1500);
      }
    };
    type();
    return () => clearTimeout(id);
  }, []);
  const status = !cloud ? (wifi ? c.statusLocal : c.statusOffline) : wifi ? c.statusCloud : c.statusFallback;
  return (
    <section className="row" aria-label="on-device dictation">
      <p className="lead">
        <b>{c.dictation.lead}</b> {c.dictation.text}
      </p>
      <div className="w">
        <div className="wline">
          <span className="wlabel">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M2 9a15 15 0 0 1 20 0" /><path d="M5.5 12.5a10 10 0 0 1 13 0" /><path d="M9 16a5 5 0 0 1 6 0" />
              <circle cx="12" cy="19.5" r="1" fill="currentColor" />
              {!wifi && <path d="M3 3l18 18" />}
            </svg>
            {c.wifi}
          </span>
          <Switch on={wifi} onChange={setWifi} label={c.wifi} />
        </div>
        <div className="wline">
          <span className="wlabel">{c.cloud}</span>
          <Switch on={cloud} onChange={setCloud} label={c.cloud} />
        </div>
        <div className="tx"><span>{typed}</span><span className="caret" /></div>
        <span className="status">{status}</span>
      </div>
    </section>
  );
}

function TrackingRow() {
  const c = useCopy().privacy;
  return (
    <section className="row" aria-label="no tracking">
      <div className="flex flex-col items-start gap-4">
        <Sticker rotate={-5} className="hidden w-[96px] md:block">
          <img src={booth} alt="A man on the phone inside a box-shaped cell phone booth costume" loading="lazy" />
        </Sticker>
        <p className="lead"><b>{c.tracking.lead}</b> {c.tracking.text}{analyticsEnabled && <> {c.siteAnalytics}</>}</p>
      </div>
      <div className="w">
        <ul className="files">
          <li><PixelSprite name="doc" size={2} /><span>analytics.log</span><span className="sz">0 bytes</span></li>
          <li><PixelSprite name="doc" size={2} /><span>telemetry.log</span><span className="sz">0 bytes</span></li>
          <li><PixelSprite name="folder" size={2} /><span>crash-reports</span><span className="sz">empty</span></li>
        </ul>
        <div className="counter"><PixelSprite name="zero" size={4} color="var(--blue-900)" /><span>{c.trackers}</span></div>
      </div>
    </section>
  );
}

const NOTES: [string, string[]][] = [
  ["standup-oct-4.md", ["# standup, oct 4", "- launch moved to friday", "- pricing copy approved", "- beta feedback goes out monday"]],
  ["maya-sync.md", ["# maya sync", "- friday works", "- send the deck by noon"]],
  ["ideas.md", ["# ideas", "- say it, don't type it", "- a tin can, but for your mac"]],
];

function NotesRow() {
  const c = useCopy().privacy;
  const [n, setN] = useState(0);
  return (
    <section className="row" aria-label="notes are files">
      <p className="lead"><b>{c.notes.lead}</b> {c.notes.text}</p>
      <div className="w finder">
        {NOTES.map(([name], i) => (
          <button key={name} className="fbtn" aria-pressed={n === i} onClick={() => setN(i)}>
            <PixelSprite name="doc" size={2} /><span>{name}</span>
          </button>
        ))}
        <pre className="preview">{NOTES[n][1].join("\n")}</pre>
        <div className="editors">{c.openWith} <span>textedit</span><span>vs code</span><span>obsidian</span><span>anything</span></div>
      </div>
    </section>
  );
}

const HISTORY: [string, string][] = [
  ["move my 3pm to friday", "2m ago"],
  ["reply to maya: friday works for me.", "14m ago"],
  ["add dentist, tuesday at 4.", "1h ago"],
  ["note: send the recap to the team.", "3h ago"],
  ["play something calm.", "yesterday"],
];

function HistoryRow() {
  const c = useCopy().privacy;
  const [rec, setRec] = useState(false);
  const [phase, setPhase] = useState<"full" | "clearing" | "empty">("full");
  const clear = () => {
    setPhase("clearing");
    setTimeout(() => setPhase("empty"), reduceMotion() ? 0 : 900);
  };
  return (
    <section className="row" aria-label="history you control">
      <p className="lead"><b>{c.history.lead}</b> {c.history.text}</p>
      <div className="w">
        <div className="wline">
          <span className="wlabel">{c.saveRecordings}</span>
          <Switch on={rec} onChange={setRec} label={c.saveRecordings} />
        </div>
        <p className="small">{rec ? c.recOn : c.recOff}</p>
        <div className="hmeta">
          <b>{phase === "full" ? c.count(50) : phase === "clearing" ? c.clearing : c.count(0)}</b>
          <GlossyButton size="sm" onClick={clear}>{c.clear}</GlossyButton>
        </div>
        {phase !== "empty" ? (
          <ul className="hist">
            {HISTORY.map(([t, ago], i) => (
              <li key={t} className={phase === "clearing" ? "go" : undefined} style={phase === "clearing" ? { animationDelay: `${i * 120}ms` } : undefined}>
                <span>{t}</span><i>{ago}</i>
              </li>
            ))}
          </ul>
        ) : (
          <p className="hist-empty">
            {c.empty}<br />
            <span className="mt-3 inline-block">
              <GlossyButton size="sm" tone="white" onClick={() => setPhase("full")}>{c.startOver}</GlossyButton>
            </span>
          </p>
        )}
      </div>
    </section>
  );
}

// The two strings between the laptop and the cloud, as points: packets hop along
// them at ~8 fps. The dotted strings are drawn from the same points.
const A: [number, number][] = [[205,112],[228,106],[252,101],[276,96],[299,92],[322,88],[346,85],[370,82],[393,80],[416,78],[440,77],[464,76],[487,76],[511,76],[534,77],[558,78],[581,80],[604,82],[628,85],[652,88],[675,92]];
const B: [number, number][] = [[205,150],[228,160],[252,169],[276,178],[299,186],[322,194],[346,201],[370,207],[393,212],[416,217],[440,221],[464,224],[487,227],[511,229],[534,231],[558,232],[581,232],[604,231],[628,230],[652,228],[675,226]];

function path(points: [number, number][]) {
  const seg = [] as { x: number; y: number; dx: number; dy: number; l: number; s: number }[];
  let tot = 0;
  for (let i = 0; i < points.length - 1; i++) {
    const dx = points[i + 1][0] - points[i][0], dy = points[i + 1][1] - points[i][1], l = Math.hypot(dx, dy);
    seg.push({ x: points[i][0], y: points[i][1], dx, dy, l, s: tot });
    tot += l;
  }
  return { seg, tot };
}
const at = (P: ReturnType<typeof path>, d: number) => {
  for (const s of P.seg) if (d <= s.s + s.l) { const u = (d - s.s) / s.l; return [s.x + s.dx * u, s.y + s.dy * u]; }
  return null;
};

function Diagram() {
  const c = useCopy().privacy;
  const packets = useRef<SVGGElement>(null);
  useEffect(() => {
    const g = packets.current!;
    const PA = path(A), PB = path(B);
    const defs = [[PA, 0, "#fff"], [PA, PA.tot * 0.34, "#8cb9ff"], [PA, PA.tot * 0.67, "#fff"], [PB, 0, "#8cb9ff"], [PB, PB.tot * 0.5, "#fff"]] as const;
    const pk = defs.map(([P, d, fill]) => {
      const el = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      el.setAttribute("width", "12"); el.setAttribute("height", "12"); el.setAttribute("fill", fill);
      el.setAttribute("stroke", "#0b1d5e"); el.setAttribute("stroke-width", "3");
      g.appendChild(el);
      return { el, P, d };
    });
    const place = (o: (typeof pk)[number]) => {
      const p = at(o.P, o.d);
      if (p) { o.el.setAttribute("x", String(Math.round(p[0] / 3) * 3 - 6)); o.el.setAttribute("y", String(Math.round(p[1] / 3) * 3 - 6)); }
    };
    pk.forEach(place);
    if (reduceMotion()) return () => g.replaceChildren();
    const hop = setInterval(() => pk.forEach((o) => { o.d += 16; if (o.d > o.P.tot) o.d -= o.P.tot; place(o); }), 125);
    return () => { clearInterval(hop); g.replaceChildren(); };
  }, []);

  const dots = (pts: [number, number][]) => pts.map(([x, y]) => <rect key={`${x}-${y}`} x={x - 4} y={y - 3} width="6" height="6" fill="rgba(255,255,255,.55)" />);
  return (
    <svg className="net-svg" viewBox="0 0 900 300" role="img" aria-label="Diagram: your mac, drawn as a cat, on the left keeps your audio. Text and commands travel along one string to an ai model, and connected services are reached along another string through their own sign-in.">
      <g shapeRendering="crispEdges">
        {dots(A)}{dots(B)}
        
      </g>
      {[[364, 37, 151, c.textNotAudio], [278, 109, 323, c.commandsApp], [356, 237, 167, c.ownSignIn]].map(([x, y, w, t]) => (
        <g key={t as string}>
          <rect x={x as number} y={y as number} width={w as number} height="26" fill="#f4f9ff" stroke="#0b1d5e" strokeWidth="3" />
          <text x={440} y={(y as number) + 18} textAnchor="middle" fontSize="13" fontWeight="600" fill="#0b1d5e">{t}</text>
        </g>
      ))}
      <text x="130" y="212" textAnchor="middle" fontSize="14" fontWeight="600" fill="#fff">{c.yourMac}</text>
      <text x="762" y="130" textAnchor="middle" fontSize="14" fontWeight="600" fill="#fff">{c.aiModel}</text>
      <text x="762" y="278" textAnchor="middle" fontSize="14" fontWeight="600" fill="#fff">{c.services}</text>
      <g ref={packets} shapeRendering="crispEdges" />
    </svg>
  );
}

export default function Privacy() {
  const c = useCopy();
  useMeta(c.privacy.title);
  return (
    <div className="priv-page">
      <main className="priv">
        <div className="wrap">
          <div className="absolute -top-14 left-0">
            <GlossyButton size="sm" tone="white" href="/">{c.common.backHome}</GlossyButton>
          </div>

          <header className="head">
            <span className="pillhint">{c.privacy.pill}</span>
            <h1>{c.privacy.h1[0]}<br />{c.privacy.h1[1]}</h1>
            <span className="stk hidden md:block" style={{ right: "34%", top: 118 }}><PixelSprite name="lock" size={5} color="#fff" /></span>
            <span className="stk hidden md:block" style={{ right: "27%", top: 60 }}><PixelSprite name="heart" size={5} color="var(--blue-300)" /></span>
            <span className="stk hidden md:block" style={{ right: "20%", top: 150 }}><PixelSprite name="folder" size={5} color="var(--blue-200)" /></span>
            <span className="kao hidden md:block" style={{ right: "30%", top: 36 }}>(¬_¬)</span>
            <Sticker rotate={7} className="absolute -top-2 right-0 hidden w-[150px] md:block">
              <img src={cat} alt="A cat with suspicious eyebrows drawn on, staring into the camera" />
            </Sticker>
          </header>

          <div className="mb-10 flex items-start gap-3 md:hidden">
            <Sticker rotate={-5} className="w-[96px]"><img src={cat} alt="A cat with suspicious eyebrows drawn on, staring into the camera" /></Sticker>
            <Sticker rotate={4} className="w-[96px]"><img src={blinds} alt="A man peeking out through window blinds" /></Sticker>
            <Sticker rotate={-3} className="w-[96px]"><img src={booth} alt="A man on the phone inside a box-shaped cell phone booth costume" /></Sticker>
          </div>

          <div className="relative">
            <MacWindow title={c.privacy.window}>
              <DictationRow />
              <TrackingRow />
              <NotesRow />
              <HistoryRow />
            </MacWindow>
            <Sticker rotate={-7} className="absolute -bottom-[78px] left-10 z-10 hidden w-[110px] md:block">
              <img src={blinds} alt="A man peeking out through window blinds" loading="lazy" />
            </Sticker>
          </div>

          <h2 className="plain">{c.privacy.plain}</h2>
          <div className="relative">
            <MacWindow title={c.privacy.diagramTitle}>
              <div className="diagram relative">
                <Diagram />
                {/* your mac, as a cat: sits where the laptop was, left of the strings */}
                <div className="absolute left-[4.5%] top-[17%] w-[16.5%] overflow-hidden rounded-[10px] border-[3px] border-blue-900 bg-white">
                  <MemeVideo src={catMac} poster={catMacPoster} alt="A cat bobbing its head up and down, standing in for your mac" className="block w-full" />
                </div>
                {/* an ai model, as the buffering cat: where the upper cloud was */}
                <div className="absolute left-[78.5%] top-[3%] w-[12.5%] overflow-hidden rounded-[10px] border-[3px] border-blue-900 bg-white">
                  <img src={aiCat} alt="A cat frozen mid-step with a loading spinner over its face, standing in for an ai model thinking" className="block aspect-square w-full object-cover" />
                </div>
                {/* connected services, as a tv of cats: where the lower cloud was */}
                <div className="absolute left-[78%] top-[49%] w-[13.5%] overflow-hidden rounded-[10px] border-[3px] border-blue-900 bg-white">
                  <img src={tv} alt="A retro television with cats pressing their faces against the screen, standing in for connected services" className="block w-full" />
                </div>
              </div>
              <div className="diagram-foot">
                <span className="tag"><PixelSprite name="lock" size={2} color="#fff" /> {c.privacy.audioStays}</span>
              </div>
              <ul className="bul">
                {c.privacy.bullets.map((b) => (
                  <li key={b}>{b}</li>
                ))}
              </ul>
            </MacWindow>
          </div>
        </div>
      </main>
    </div>
  );
}
