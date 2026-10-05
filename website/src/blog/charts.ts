// Inline SVG charts as strings, so the page and the build-time static HTML share them. Text
// follows currentColor and the background is transparent, so a chart sits on any surface.
const esc = (t: string) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const SHELL = `style="max-width:100%;height:auto;font-family:Inter,system-ui,sans-serif" role="img"`;
const BLUE = "#2f7cf0";
const NAVY = "#0b1d5e";

export type Row = { label: string; value: number; color?: string; note?: string };

// Horizontal bars, one per row, scaled to the largest value.
export function hbar(title: string, desc: string, rows: Row[], source: string, unit = "") {
  const max = Math.max(...rows.map((r) => r.value));
  const h = 74 + rows.length * 46;
  const bars = rows
    .map((r, i) => {
      const y = 62 + i * 46;
      const w = Math.round((r.value / max) * 300);
      return `<text x="188" y="${y + 19}" text-anchor="end" font-size="13" fill="currentColor" opacity="0.8">${esc(r.label)}</text>
  <rect x="198" y="${y}" width="${w}" height="28" rx="4" fill="${r.color ?? BLUE}"/>
  <text x="${198 + w + 8}" y="${y + 19}" font-size="14" font-weight="700" fill="currentColor">${r.value}${unit}</text>`;
    })
    .join("\n  ");
  return `<svg viewBox="0 0 560 ${h}" ${SHELL} aria-label="${esc(desc)}"><title>${esc(title)}</title><desc>${esc(desc)}</desc>
  <text x="14" y="26" font-size="15" font-weight="600" fill="currentColor">${esc(title)}</text>
  ${bars}
  <text x="14" y="${h - 10}" font-size="10" fill="currentColor" opacity="0.5">${esc(source)}</text>
</svg>`;
}

// Two series side by side across a few groups.
export function gbar(title: string, desc: string, groups: string[], series: { name: string; color: string; values: number[] }[], source: string, unit = "%") {
  const max = Math.max(...series.flatMap((s) => s.values));
  const base = 280, top = 70, plot = base - top;
  const gw = 220, bw = 70;
  const out = groups.map((g, gi) => {
    const x0 = 70 + gi * (gw + 30);
    const bars = series.map((s, si) => {
      const v = s.values[gi];
      const hgt = Math.round((v / max) * plot);
      const x = x0 + 30 + si * (bw + 10);
      return `<rect x="${x}" y="${base - hgt}" width="${bw}" height="${hgt}" rx="4" fill="${s.color}"/>
  <text x="${x + bw / 2}" y="${base - hgt - 7}" text-anchor="middle" font-size="14" font-weight="700" fill="currentColor">${v.toFixed(2)}${unit}</text>`;
    });
    return `${bars.join("\n  ")}
  <text x="${x0 + gw / 2 - 5}" y="${base + 22}" text-anchor="middle" font-size="13" fill="currentColor" opacity="0.8">${esc(g)}</text>`;
  });
  const legend = series.map((s, i) => `<rect x="${14 + i * 140}" y="38" width="12" height="12" rx="2" fill="${s.color}"/><text x="${32 + i * 140}" y="48" font-size="12" fill="currentColor" opacity="0.8">${esc(s.name)}</text>`).join("");
  return `<svg viewBox="0 0 560 330" ${SHELL} aria-label="${esc(desc)}"><title>${esc(title)}</title><desc>${esc(desc)}</desc>
  <text x="14" y="22" font-size="15" font-weight="600" fill="currentColor">${esc(title)}</text>
  ${legend}
  <line x1="60" y1="${base}" x2="540" y2="${base}" stroke="currentColor" opacity="0.3"/>
  ${out.join("\n  ")}
  <text x="14" y="322" font-size="10" fill="currentColor" opacity="0.5">${esc(source)}</text>
</svg>`;
}

export const chartColors = { blue: BLUE, navy: NAVY };
