// The evals dashboard. One fetch of /api/data; every figure is computed here
// from the saved eval runs and the app's own traces. Plain DOM and template
// strings: no build step, nothing to install.

const TABS = [
  ['overview', 'Overview'],
  ['evals', 'Command evals'],
  ['commands', 'Real commands'],
  ['browser', 'Browser tasks'],
  ['dictation', 'Dictation'],
  ['speed', 'Speed'],
  ['tokens', 'Tokens & cache'],
]

const state = {
  m: null,
  tab: TABS.some(([id]) => id === location.hash.slice(1)) ? location.hash.slice(1) : 'overview',
  run: null,
  base: undefined, // undefined: the brain's previous run; '': none; else a file name
  caseFilter: 'all',
  caseQuery: '',
  rows: [],
  cmdFilter: 'all',
  speedKind: 'dictation',
}

// ── Small helpers ───────────────────────────────────────────────────────────

const $ = (id) => document.getElementById(id)
const esc = (v) => String(v ?? '').replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`)
const sum = (xs) => xs.reduce((a, b) => a + b, 0)
const plural = (n, word, many = `${word}s`) => `${n} ${n === 1 ? word : many}`
const pct = (k, n, digits = 0) => (n ? `${((100 * k) / n).toFixed(digits)}%` : '—')
// Nearest-rank percentile: every value shown is one that really happened.
function q(xs, p) {
  if (!xs.length) return null
  const s = [...xs].sort((a, b) => a - b)
  return s[Math.min(s.length - 1, Math.max(0, Math.ceil(p * s.length) - 1))]
}
const dur = (ms) =>
  ms == null ? '—'
  : ms < 1000 ? `${Math.round(ms)} ms`
  : ms < 60000 ? `${(ms / 1000).toFixed(ms < 10000 ? 2 : 1)} s`
  : `${(ms / 60000).toFixed(1)} min`
const num = (n) => (n == null ? '—' : n >= 10000 ? `${Math.round(n / 1000)}k` : n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(Math.round(n)))
const when = (ms) => new Date(ms).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })
const dayLabel = (ms) => new Date(ms).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
const dayKey = (ms) => new Date(ms).toLocaleDateString('en-CA')
// Local noon, so the day survives any time zone's round trip.
const dayStart = (key) => Date.parse(`${key}T12:00`)
const ago = (ms) => {
  const h = (Date.now() - ms) / 3600000
  return h < 1 ? 'within the hour' : h < 24 ? `${Math.round(h)} h ago` : `${Math.round(h / 24)} d ago`
}
const list = (xs, n = 3) => xs.slice(0, n).map((x) => `“${x}”`).join(', ') + (xs.length > n ? ` and ${xs.length - n} more` : '')
const tally = (xs, key) => {
  const m = new Map()
  for (const x of xs) m.set(key(x), (m.get(key(x)) ?? 0) + 1)
  return [...m].sort((a, b) => b[1] - a[1])
}
// Wilson score interval: honest bounds for a pass rate over a few dozen cases.
function wilson(k, n, z = 1.96) {
  if (!n) return [0, 1]
  const p = k / n
  const d = 1 + (z * z) / n
  const c = p + (z * z) / (2 * n)
  const m = z * Math.sqrt((p * (1 - p)) / n + (z * z) / (4 * n * n))
  return [Math.max(0, (c - m) / d), Math.min(1, (c + m) / d)]
}

// ── The model: raw rows into traces, runs and cases ─────────────────────────

const WAITS = new Set(['confirm', 'ask_user']) // the user's time, not Aloud's
const MODEL_STAGES = new Set(['tool_select', 'tool_select_retry', 'verify', 'cleanup', 'transform', 'ask_answer', 'stream_finish'])
const DICTATION_STAGES = ['finalize_audio', 'transcribe', 'cleanup', 'paste']

function build(raw) {
  const byUtterance = new Map(raw.history.filter((h) => h.utteranceId != null).map((h) => [h.utteranceId, h]))
  const tasks = raw.browserTasks.map((b) => ({ ...b, startAt: b.endAt - (b.ms ?? 0) }))
  const traces = raw.traces.map((t) => shape(t, byUtterance.get(t.id))).filter(Boolean)
  for (const t of traces) {
    if (t.kind !== 'command') continue
    t.browser = tasks.filter((b) => b.startAt >= t.at - 1000 && b.startAt <= t.at + t.ms)
    // Runs recorded before `agent_run` carried its route: read it off the spans.
    t.route ??= t.rounds ? 'brain' : t.browser.length ? 'web' : t.tools.length ? 'fast' : 'answer'
  }
  const matchers = raw.evalSet.flatMap((c) => [c.expected?.[0]?.callsTool?.name].flat()).filter(Boolean)
  return {
    raw,
    traces,
    tasks,
    runs: raw.evalRuns.map(shapeRun),
    dictations: traces.filter((t) => t.kind === 'dictation'),
    commands: traces.filter((t) => t.kind === 'command'),
    covered: (tool) => matchers.some((m) => (m.endsWith('_') ? tool.startsWith(m) : tool === m)),
  }
}

function shape(t, hist) {
  const root = t.spans.find((s) => s.stage === 'utterance')
  if (!root) return null // still running, or its root was pruned
  const f = root.fields
  const of = (stage) => t.spans.filter((s) => s.stage === stage)
  const kind = f.mode === 'rewrite' ? 'rewrite' : f.mode === 'dictation' && f.outcome !== 'command' ? 'dictation' : 'command'
  const rounds = of('tool_select')
  const usage = rounds.map((s) => s.fields.usage).filter(Boolean)
  const tools = t.spans
    .filter((s) => s.stage === 'tool_exec' || s.stage === 'composio_exec')
    .map((s) => ({ name: s.fields.tool ?? s.stage, kit: s.fields.toolkit ?? '', ok: s.ok, ms: s.ms }))
  const waiting = sum(t.spans.filter((s) => WAITS.has(s.stage)).map((s) => s.ms))
  const run = of('agent_run')[0]
  return {
    id: t.id,
    at: root.at,
    ms: root.ms,
    machineMs: root.ms - waiting,
    waiting,
    kind,
    mode: f.mode,
    ok: root.ok,
    outcome:
      kind === 'command' ? (!root.ok ? 'failed' : tools.some((x) => !x.ok) ? 'tool failed' : 'done')
      : f.outcome ?? (root.ok ? 'unrecorded' : 'error'),
    spokenMs: f.spoken_ms ?? null,
    route: run?.fields.route ?? null,
    toolCount: run?.fields.tool_count ?? null,
    rounds: rounds.length,
    tools,
    asked: of('ask_user').length > 0,
    models: rounds.map((s) => s.fields.model).filter(Boolean),
    usage,
    tokens: {
      prompt: sum(usage.map((u) => u.prompt_tokens ?? 0)),
      cached: sum(usage.map((u) => u.prompt_tokens_details?.cached_tokens ?? 0)),
      completion: sum(usage.map((u) => u.completion_tokens ?? 0)),
    },
    engine: of('transcribe')[0]?.fields.engine ?? null,
    stage: (name) => (of(name).length ? sum(of(name).map((s) => s.ms)) : null),
    spans: t.spans,
    hist,
    browser: [],
  }
}

function category(c) {
  const want = c.expected?.[0]
  if (want?.not) return 'Answer only'
  if (c.fast) return 'Fast path'
  const name = [want?.callsTool?.name].flat()[0] ?? ''
  if (name === 'ALOUD_BROWSER_TASK') return 'Browser'
  return name.startsWith('ALOUD_') ? 'Mac tools' : 'Connected services'
}

// The harness's own failure messages, named for what to go and fix.
const failureKind = (why = '') =>
  why.startsWith('fast path') ? 'Fast path misroute'
  : why.startsWith('error:') ? 'API error'
  : why.startsWith('answer expected') ? 'Called a tool, should answer'
  : why.startsWith('answered without') ? 'Answered, should call a tool'
  : why.startsWith('loaded') ? 'Loaded the wrong service'
  : /without `|` = /.test(why) ? 'Right tool, wrong arguments'
  : why.startsWith('called') ? 'Wrong tool'
  : 'Other'

function shapeRun(run) {
  const cases = run.cases.map((c) => {
    const attempts = c.attempts ?? []
    const scored = attempts.filter((a) => a.verdict !== 'skip')
    const passes = scored.filter((a) => a.verdict === 'pass').length
    const status = !scored.length ? 'skip' : passes === scored.length ? 'pass' : passes ? 'flaky' : 'fail'
    const shown = attempts.find((a) => a.verdict === 'fail') ?? attempts[0] ?? {}
    return {
      ...c,
      key: `${c.input}\u0000${c.selection ?? ''}`,
      attempts,
      status,
      passes,
      shown,
      category: category(c),
      failure: status === 'fail' || status === 'flaky' ? failureKind(shown.why) : null,
    }
  })
  const count = (s) => cases.filter((c) => c.status === s).length
  const at = Date.parse(run.startedAt)
  return {
    ...run,
    at,
    cases,
    pass: count('pass'),
    fail: count('fail'),
    flaky: count('flaky'),
    skip: count('skip'),
    scored: cases.length - count('skip'),
    brainMs: cases.flatMap((c) => c.attempts.filter((a) => a.route === 'brain' && a.verdict !== 'skip').map((a) => a.ms)),
  }
}

const gitLabel = (run) => (run.git?.sha ? `${run.git.branch || 'detached'}@${run.git.sha}${run.git.dirty ? '*' : ''}` : 'no git info')
const baselineFor = (run) => state.m.runs.find((r) => r.brain === run.brain && r.at < run.at) ?? null
const latestPerBrain = (runs) => [...new Map([...runs].reverse().map((r) => [r.brain, r])).values()].sort((a, b) => b.at - a.at)

function compare(run, base) {
  const before = new Map((base?.cases ?? []).map((c) => [c.key, c]))
  return run.cases.map((c) => {
    const b = before.get(c.key)
    const was = b?.status ?? null
    const broke = c.status === 'fail' || c.status === 'flaky'
    const change =
      !base ? null
      : !b ? 'new'
      : was === 'pass' && broke ? 'regressed'
      : (was === 'fail' || was === 'flaky') && c.status === 'pass' ? 'fixed'
      : null
    return { ...c, was, change }
  })
}

// ── Derived metrics shared by several views ─────────────────────────────────

function toolHealth(m) {
  const by = new Map()
  for (const t of m.commands)
    for (const x of t.tools) {
      const row = by.get(x.name) ?? { name: x.name, kit: x.kit, calls: 0, failed: 0, ms: [] }
      row.calls++
      row.failed += x.ok ? 0 : 1
      row.ms.push(x.ms)
      by.set(x.name, row)
    }
  return [...by.values()].sort((a, b) => b.failed - a.failed || b.calls - a.calls)
}

const PUNCTUATION_ONLY = /^[\p{P}\p{S}\s]*$/u
// ponytail: a hand list of whisper's favourite inventions on silence; a fixed
// silent-audio eval set is the upgrade path.
const SILENCE_WORDS = new Set(['you', 'thank you', 'thanks for watching', 'bye', 'thank you for watching'])
const words = (s) => s.split(/\s+/).filter(Boolean)
const norm = (w) => w.toLowerCase().replace(/[^\p{L}\p{N}']/gu, '')

// Word-level diff over normalised words, so cleanup's punctuation and
// capitals are not counted as edits.
function diffWords(a, b) {
  const A = words(a)
  const B = words(b)
  const na = A.map(norm)
  const nb = B.map(norm)
  // ponytail: O(n·m) LCS; long meeting-length pastes are skipped, not diffed.
  if (A.length * B.length > 250000) return null
  const L = Array.from({ length: A.length + 1 }, () => new Uint16Array(B.length + 1))
  for (let i = A.length - 1; i >= 0; i--)
    for (let j = B.length - 1; j >= 0; j--) L[i][j] = na[i] === nb[j] ? L[i + 1][j + 1] + 1 : Math.max(L[i + 1][j], L[i][j + 1])
  const ops = []
  let i = 0
  let j = 0
  while (i < A.length && j < B.length) {
    if (na[i] === nb[j]) ops.push(['=', B[j++], i++])
    else if (L[i + 1][j] >= L[i][j + 1]) ops.push(['-', A[i++]])
    else ops.push(['+', B[j++]])
  }
  while (i < A.length) ops.push(['-', A[i++]])
  while (j < B.length) ops.push(['+', B[j++]])
  const edits = ops.filter((o) => o[0] !== '=').length
  return { ops, edits, rate: edits / Math.max(1, A.length, B.length) }
}

function cleanupRows(m) {
  return m.raw.history.map((h) => {
    const diff = diffWords(h.raw, h.final)
    const final = h.final.trim()
    const flags = []
    if (PUNCTUATION_ONLY.test(final)) flags.push('pasted only punctuation')
    else if (SILENCE_WORDS.has(norm(final.replace(/\s+/g, ' ')).trim()) || SILENCE_WORDS.has(final.toLowerCase().replace(/[.!?]/g, '').trim()))
      flags.push('likely invented on silence')
    if (words(h.raw).length >= 8 && words(h.final).length < 0.6 * words(h.raw).length) flags.push('cleanup dropped words')
    return { ...h, diff, flags }
  })
}

const modelRounds = (m) => m.commands.flatMap((t) => t.spans.filter((s) => s.stage === 'tool_select'))
// The brain now in use: the newest round's model. Older models' rounds would
// otherwise blend a retired brain's cache behaviour into today's figure.
function currentBrain(m) {
  const rounds = modelRounds(m)
  const model = rounds.reduce((a, b) => (b.at > a.at ? b : a), rounds[0] ?? { fields: {} }).fields.model ?? null
  return { model, rounds: rounds.filter((s) => s.fields.model === model) }
}
const cacheHit = (usages) => {
  const prompt = sum(usages.map((u) => u.prompt_tokens ?? 0))
  return prompt ? sum(usages.map((u) => u.prompt_tokens_details?.cached_tokens ?? 0)) / prompt : null
}

// ── Review notes ────────────────────────────────────────────────────────────

function findings(m) {
  const out = []
  const add = (level, title, body, tab) => out.push({ level, title, body, tab })

  if (!m.runs.length) add('info', 'No eval runs saved yet', 'Run the command eval (see Command evals); each brain’s run lands here.', 'evals')
  for (const run of latestPerBrain(m.runs)) {
    const base = baselineFor(run)
    const rows = compare(run, base)
    const regressed = rows.filter((r) => r.change === 'regressed')
    if (regressed.length)
      add('bad', `${plural(regressed.length, 'eval case')} regressed on ${run.brain}`, `${list(regressed.map((r) => r.input))} passed in the run before (${gitLabel(base)}).`, 'evals')
    const failing = run.cases.filter((c) => c.failure)
    if (failing.length) {
      const [kind] = tally(failing, (c) => c.failure)[0]
      add('warn', `${run.brain} misroutes ${failing.length} of ${run.scored} cases`, `Mostly “${kind.toLowerCase()}”: ${list(failing.map((c) => c.input))}.`, 'evals')
    }
    const skipped = run.cases.filter((c) => c.status === 'skip')
    if (skipped.length) {
      const tools = [...new Set(skipped.map((c) => (c.shown.why ?? '').replace(/ not offered here$/, '').replaceAll('"', '')))]
      add('info', `${plural(skipped.length, 'case')} untested on ${run.brain}`, `Their tool was not offered when the eval ran (${tools.join(', ')}), so they prove nothing either way.`, 'evals')
    }
  }
  const newest = m.runs[0]
  if (newest && newest.k === 1)
    add('info', 'Each case ran once', 'One pass hides flakiness. EVAL_K=3 runs each brain case three times and counts it only if all three pass.', 'evals')
  if (newest && Date.now() - newest.at > 3 * 86400000) add('info', `Last eval run was ${ago(newest.at)}`, 'Run it again after any prompt, tool or model change.', 'evals')

  const health = toolHealth(m)
  for (const t of health.filter((t) => t.failed >= 2).slice(0, 3))
    add(t.failed / t.calls >= 0.25 ? 'bad' : 'warn', `${t.name} failed ${t.failed} of ${t.calls} real calls`, 'In real use, not the eval. Open Real commands and filter to tool failures to read the traces.', 'commands')
  const uncovered = health.filter((t) => t.calls >= 2 && !m.covered(t.name))
  if (uncovered.length)
    add('warn', `${plural(uncovered.length, 'tool')} you use have no eval case`, `${list(uncovered.map((t) => t.name), 4)}. Add a case for each to commands.jsonl so a prompt change cannot quietly break them.`, 'commands')

  const brainRuns = m.commands.filter((t) => t.route === 'brain')
  const long = brainRuns.filter((t) => t.rounds >= 3)
  if (brainRuns.length && long.length / brainRuns.length >= 0.15)
    add('warn', `${pct(long.length, brainRuns.length)} of brain commands took 3+ model rounds`, 'Each round is another model call. Read a few: a tool whose result the brain cannot use, or a missing direct tool, usually explains it.', 'commands')
  const failed = m.commands.filter((t) => t.outcome === 'failed')
  if (failed.length) add('warn', `${plural(failed.length, 'command')} ended in an error`, `Most recent ${ago(failed[0].at)}.`, 'commands')

  const tasks = m.tasks
  const bad = tasks.filter((t) => t.status !== 'done')
  if (tasks.length && bad.length / tasks.length >= 0.2)
    add('bad', `${pct(bad.length, tasks.length)} of browser tasks got stuck or failed`, `${tally(bad, (t) => t.status).map(([s, n]) => `${n} ${s}`).join(', ')} of ${tasks.length}. E.g. ${list(bad.map((t) => t.goal), 2)}.`, 'browser')

  const d = m.dictations
  const longSilent = d.filter((t) => t.outcome === 'nospeech' && t.spokenMs >= 2000)
  if (longSilent.length >= 3)
    add('warn', `${plural(longSilent.length, 'time')} you held the key 2 s or more and got nothing`, 'Either nobody spoke or the silence gate missed speech. If recordings are kept, listen to a few.', 'dictation')
  const tinyErrors = d.filter((t) => t.outcome === 'error' && t.spokenMs != null && t.spokenMs < 50)
  if (tinyErrors.length)
    add('warn', `${plural(tinyErrors.length, 'press', 'presses')} too short to record raised an error`, 'A press under 50 ms has no audio to transcribe; it should end quietly as no speech instead of erroring.', 'dictation')
  const flagged = cleanupRows(m).filter((r) => r.flags.length)
  if (flagged.length)
    add('bad', `${plural(flagged.length, 'dictation')} pasted something suspicious`, `${tally(flagged, (r) => r.flags[0]).map(([f, n]) => `${n} ${f}`).join(', ')}. Pasted text that nobody said is the worst failure for a paste-anywhere tool.`, 'dictation')
  const pasted = d.filter((t) => t.outcome === 'pasted')
  const p95 = q(pasted.map((t) => t.ms), 0.95)
  if (p95 > 1500) {
    const slowest = DICTATION_STAGES.map((s) => [s, q(pasted.map((t) => t.stage(s)).filter((v) => v != null), 0.95)]).sort((a, b) => b[1] - a[1])[0]
    add('warn', `1 in 20 dictations takes over ${dur(p95)} to paste`, `The slowest stage at the 95th percentile is ${slowest[0]} (${dur(slowest[1])}).`, 'speed')
  }

  const brain = currentBrain(m)
  const hit = cacheHit(brain.rounds.map((s) => s.fields.usage).filter(Boolean))
  if (hit != null && hit < 0.5)
    add('warn', `Only ${pct(hit, 1)} of ${brain.model}’s prompt tokens hit the cache`, 'A changing prefix (time, tool order, context first) breaks prompt caching; keep the stable part first.', 'tokens')
  const failedOver = m.commands.filter((t) => new Set(t.models).size > 1)
  if (failedOver.length) add('info', `${plural(failedOver.length, 'command')} failed over to another brain mid-run`, `Most recent ${ago(failedOver[0].at)}. Open Tokens & cache for which brain answered.`, 'tokens')

  const order = { bad: 0, warn: 1, info: 2, ok: 3 }
  return out.sort((a, b) => order[a.level] - order[b.level])
}

// ── Rendering primitives ────────────────────────────────────────────────────

const LEVEL = { bad: 'Needs attention', warn: 'Worth a look', info: 'Note', ok: 'Good' }
const STATUS_BADGE = { pass: 'ok', fail: 'bad', flaky: 'warn', skip: 'muted' }
const OUTCOME_BADGE = { pasted: 'ok', done: 'ok', nospeech: 'muted', cancelled: 'muted', unrecorded: 'muted', error: 'bad', failed: 'bad', 'tool failed': 'warn', command: 'info', stuck: 'warn' }
const badge = (text, kind = OUTCOME_BADGE[text] ?? '') => `<span class="badge ${kind}">${esc(text)}</span>`

const panel = (title, sub, body, extra = '') => `<section class="panel"><div class="panel-head"><div><h3>${esc(title)}</h3>${sub ? `<p class="sub">${esc(sub)}</p>` : ''}</div>${extra}</div>${body}</section>`
const mini = (value, label) => `<div class="mini"><b>${value}</b><span>${label}</span></div>`
const empty = (text) => `<div class="empty">${text}</div>`

function bars(rows, { max, colors = ['var(--chart-1)'] } = {}) {
  const top = max ?? Math.max(1, ...rows.map((r) => sum([r.values].flat())))
  return `<div class="bars">${rows
    .map(
      (r) => `<div class="bar-row"><span class="wrap">${esc(r.label)}</span><div class="bar-track">${[r.values]
        .flat()
        .map((v, i) => `<span style="width:${(100 * v) / top}%;background:${colors[i % colors.length]}"></span>`)
        .join('')}</div><span class="num">${r.note ?? ''}</span></div>`,
    )
    .join('')}</div>`
}

function hist(buckets) {
  const top = Math.max(1, ...buckets.map((b) => b.n))
  return `<div class="hist">${buckets
    .map((b) => `<div class="col" title="${esc(b.label)}: ${b.n}"><span class="val">${b.n || ''}</span><div class="fill" style="height:${(100 * b.n) / top}%;${b.color ? `background:${b.color}` : ''}"></div></div>`)
    .join('')}</div><div class="hist-labels">${buckets.map((b) => `<span>${esc(b.label)}</span>`).join('')}</div>`
}

// Buckets are [lo, hi); `label(lo, hi)` names one.
const bucket = (values, edges, label) =>
  edges.map((lo, i) => {
    const hi = edges[i + 1] ?? Infinity
    return { label: label(lo, hi), n: values.filter((v) => v >= lo && v < hi).length }
  })
const secs = (lo, hi) => (hi === Infinity ? `${lo / 1000}s+` : `${lo / 1000}–${hi / 1000}s`)
// Whole numbers, labelled by the counts a bucket holds: 0, 1–2, 12+.
const counts = (lo, hi) => (hi === Infinity ? `${lo}+` : hi - lo === 1 ? String(lo) : `${lo}–${hi - 1}`)

function lineChart(series, { fmt = (v) => `${Math.round(v)}%`, yMin = 0, yMax = 100 } = {}) {
  const xs = series.flatMap((s) => s.points.map((p) => p.x))
  if (!xs.length) return empty('Nothing to plot yet.')
  const W = 640
  const H = 200
  const [L, R, T, B] = [44, 16, 12, 26]
  let [x0, x1] = [Math.min(...xs), Math.max(...xs)]
  if (x0 === x1) [x0, x1] = [x0 - 3600000, x1 + 3600000]
  const X = (x) => L + ((x - x0) / (x1 - x0)) * (W - L - R)
  const Y = (y) => T + (1 - (y - yMin) / (yMax - yMin || 1)) * (H - T - B)
  const grid = [yMin, (yMin + yMax) / 2, yMax]
    .map((y) => `<line x1="${L}" x2="${W - R}" y1="${Y(y)}" y2="${Y(y)}" stroke="var(--border)"/><text x="${L - 8}" y="${Y(y) + 4}" text-anchor="end">${esc(fmt(y))}</text>`)
    .join('')
  const lines = series
    .map(
      (s) =>
        `<polyline fill="none" stroke="${s.color}" stroke-width="2" points="${s.points.map((p) => `${X(p.x)},${Y(p.y)}`).join(' ')}"/>` +
        s.points.map((p) => `<circle cx="${X(p.x)}" cy="${Y(p.y)}" r="4" fill="var(--surface)" stroke="${s.color}" stroke-width="2"><title>${esc(p.title ?? fmt(p.y))}</title></circle>`).join(''),
    )
    .join('')
  const axis = `<text x="${L}" y="${H - 6}">${esc(dayLabel(x0))}</text><text x="${W - R}" y="${H - 6}" text-anchor="end">${esc(dayLabel(x1))}</text>`
  const legend = `<div class="legend">${series.map((s) => `<span><i style="background:${s.color}"></i>${esc(s.name)}</span>`).join('')}</div>`
  return `<svg class="line" viewBox="0 0 ${W} ${H}" width="100%" role="img" aria-label="Line chart">${grid}${lines}${axis}</svg>${legend}`
}

const SERIES_COLORS = ['var(--chart-1)', 'var(--chart-2)', 'var(--chart-3)', 'var(--chart-4)']

function waterfall(t) {
  const end = Math.max(t.at + t.ms, ...t.spans.map((s) => s.at + s.ms))
  const span = Math.max(1, end - t.at)
  return `<div class="wf">${t.spans
    .map((s) => {
      const cls = s.stage === 'utterance' ? 'root' : !s.ok ? 'fail' : WAITS.has(s.stage) ? 'wait' : MODEL_STAGES.has(s.stage) ? 'model' : ''
      const name = s.fields.tool ?? (s.stage === 'tool_select' ? `${s.stage} #${(s.fields.round ?? 0) + 1}` : s.stage)
      return `<div class="wf-row"><span class="wf-name" title="${esc(name)}">${esc(name)}</span><div class="wf-track"><div class="wf-bar ${cls}" style="left:${(100 * (s.at - t.at)) / span}%;width:${(100 * s.ms) / span}%"></div></div><span class="num">${dur(s.ms)}</span></div>`
    })
    .join('')}</div><div class="legend"><span><i style="background:var(--chart-1)"></i>Aloud</span><span><i style="background:var(--chart-2)"></i>model call</span><span><i style="background:var(--peach)"></i>waiting on you</span><span><i style="background:var(--bad)"></i>failed</span></div>`
}

const diffHtml = (d) =>
  d ? d.ops.map(([op, w]) => (op === '=' ? esc(w) : op === '-' ? `<del>${esc(w)}</del>` : `<ins>${esc(w)}</ins>`)).join(' ') : '<span class="muted">Too long to diff.</span>'

const toolChips = (tools) =>
  tools.length ? `<span class="chips">${tools.map((x) => `<span class="chip ${x.ok ? '' : 'bad'}" title="${esc(x.kit)} · ${dur(x.ms)}">${esc(x.name.replace(/^ALOUD_/, ''))}${x.ok ? '' : ' ✗'}</span>`).join('')}</span>` : '<span class="muted">no tool</span>'

// ── The page frame ──────────────────────────────────────────────────────────

function render() {
  const m = state.m
  document.querySelectorAll('.nav [data-tab]').forEach((b) => b.setAttribute('aria-current', b.dataset.tab === (state.tab === 'evals' ? 'evals' : 'overview') ? 'page' : 'false'))
  $('runCount').textContent = m.runs.length || ''
  const first = m.traces.at(-1)
  const views = { overview, evals, commands, browser, dictation, speed, tokens }
  $('main').innerHTML = `
    <div class="heading-row"><div>
      <div class="eyebrow">Aloud / Eval studio</div>
      <h1>How Aloud is doing.</h1>
      <p class="sub">${m.runs.length ? `${plural(m.runs.length, 'eval run')} · ` : ''}${plural(m.traces.length, 'real utterance')}${first ? ` since ${dayLabel(first.at)}` : ''} · traced on this Mac</p>
    </div></div>
    ${glance(m)}
    <div class="tabs" role="tablist">${TABS.map(([id, label]) => `<button class="tab" role="tab" data-tab="${id}" aria-selected="${state.tab === id}">${label}</button>`).join('')}</div>
    <div id="view">${views[state.tab](m)}</div>`
}

function glance(m) {
  const primary = latestPerBrain(m.runs).find((r) => r.brain === 'cerebras') ?? latestPerBrain(m.runs)[0]
  const notes = findings(m)
  const urgent = notes.filter((n) => n.level === 'bad').length
  const [lo, hi] = primary ? wilson(primary.pass, primary.scored) : [0, 0]
  const base = primary && baselineFor(primary)
  const delta = base ? primary.pass / primary.scored - base.pass / base.scored : null
  const cmds = m.commands
  const pasted = m.dictations.filter((t) => t.outcome === 'pasted')
  const brain = currentBrain(m)
  const hit = cacheHit(brain.rounds.map((s) => s.fields.usage).filter(Boolean))
  const perCommand = cmds.filter((t) => t.tokens.prompt && t.models.every((x) => x === brain.model)).map((t) => t.tokens.prompt + t.tokens.completion)
  const headline = primary
    ? `${primary.pass} of ${primary.scored} eval commands routed right.`
    : `${pct(cmds.filter((t) => t.outcome !== 'failed').length, cmds.length)} of real commands finished.`
  return `<section class="summary">
    <div class="glance"><div>
      <div class="eyebrow">At a glance</div>
      <h2>${esc(headline)}</h2>
      <p class="sub">${primary ? `${esc(primary.brain)} (${esc(primary.model)}) · ${esc(gitLabel(primary))} · ${ago(primary.at)}. ` : ''}Eval runs are curated; real use is noisy. They are shown apart, never blended.</p>
    </div><div class="glance-side"><div class="metric">${m.traces.length}</div><div>utterances traced</div><div class="caption">${m.commands.length} commands · ${m.dictations.length} dictations</div></div></div>
    ${urgent ? `<div class="banner">⚠ <span><b>${plural(urgent, 'finding')} need${urgent === 1 ? 's' : ''} attention.</b> ${esc(notes[0].title)}.</span> <button class="text-button" data-tab="overview">Review notes →</button></div>` : ''}
    <div class="stats">
      <button class="stat" data-tab="evals"><span class="stat-label">Eval cases passing <span>↗</span></span>
        <div class="metric">${primary ? `${primary.pass}<small>/${primary.scored}</small>` : '—'}</div>
        <span class="stat-note">${primary ? `95% CI ${pct(lo, 1)}–${pct(hi, 1)}${delta != null ? ` · ${delta === 0 ? 'same as' : delta > 0 ? `▲ ${pct(delta, 1, 1)} vs` : `▼ ${pct(-delta, 1, 1)} vs`} previous` : ''}` : 'No eval run saved yet'}</span></button>
      <button class="stat" data-tab="commands"><span class="stat-label">Real commands finished <span>↗</span></span>
        <div class="metric">${pct(cmds.filter((t) => t.outcome !== 'failed').length, cmds.length)}</div>
        <span class="stat-note">${cmds.filter((t) => t.outcome === 'tool failed').length} of ${cmds.length} finished after a tool failed</span></button>
      <button class="stat" data-tab="speed"><span class="stat-label">Release to paste, p95 <span>↗</span></span>
        <div class="metric">${dur(q(pasted.map((t) => t.ms), 0.95))}</div>
        <span class="stat-note">Typical ${dur(q(pasted.map((t) => t.ms), 0.5))} · ${pasted.length} pastes</span></button>
      <button class="stat" data-tab="tokens"><span class="stat-label">Prompt cache hit <span>↗</span></span>
        <div class="metric">${hit == null ? '—' : pct(hit, 1)}</div>
        <span class="stat-note">${esc(brain.model ?? 'no brain yet')} · typical command ${num(q(perCommand, 0.5))} tokens · p95 ${num(q(perCommand, 0.95))}</span></button>
    </div></section>`
}

// ── Overview ────────────────────────────────────────────────────────────────

function overview(m) {
  const notes = findings(m)
  const brains = [...new Set(m.runs.map((r) => r.brain))]
  const trend = lineChart(
    brains.map((brain, i) => ({
      name: brain,
      color: SERIES_COLORS[i % SERIES_COLORS.length],
      points: m.runs
        .filter((r) => r.brain === brain && r.scored)
        .map((r) => ({ x: r.at, y: (100 * r.pass) / r.scored, title: `${brain} · ${when(r.at)} · ${r.pass}/${r.scored} · ${gitLabel(r)}` }))
        .reverse(),
    })),
    { yMin: Math.max(0, Math.floor((Math.min(100, ...m.runs.filter((r) => r.scored).map((r) => (100 * r.pass) / r.scored)) - 10) / 10) * 10) },
  )
  return `<div class="grid main-aside">
    <div class="grid">
      ${panel('Is routing getting better?', 'Eval pass rate per saved run, by brain. With ~45 cases one case moves it about two points: read which cases flipped, not the decimals.', trend)}
      ${panel('What you use Aloud for', 'Utterances on each of the last 14 days you used it.', dailyMix(m))}
    </div>
    <aside class="panel detail"><div class="eyebrow">Review notes</div><h2 style="margin:8px 0 12px">What deserves a closer look</h2>
      ${notes.map((n) => `<button class="note" data-tab="${n.tab}"><span class="eyebrow level-${n.level}">${LEVEL[n.level]}</span><h4>${esc(n.title)}</h4><p>${esc(n.body)}</p></button>`).join('') || '<p class="sub">Nothing stands out.</p>'}
    </aside></div>`
}

function dailyMix(m) {
  // Active days only: use comes in bursts, and a calendar of empty columns hides the bursts.
  const days = [...new Set(m.traces.map((t) => dayKey(t.at)))].sort().slice(-14)
  const groups = [
    ['Pasted', 'var(--chart-3)', (t) => t.kind === 'dictation' && t.outcome === 'pasted'],
    ['Nothing pasted', 'var(--border-strong)', (t) => t.kind === 'dictation' && t.outcome !== 'pasted'],
    ['Command done', 'var(--chart-1)', (t) => t.kind === 'command' && t.outcome !== 'failed'],
    ['Command failed', 'var(--bad)', (t) => t.kind === 'command' && t.outcome === 'failed'],
    ['Rewrite', 'var(--chart-2)', (t) => t.kind === 'rewrite'],
  ]
  const byDay = new Map(days.map((d) => [d, []]))
  for (const t of m.traces) byDay.get(dayKey(t.at))?.push(t)
  const top = Math.max(1, ...[...byDay.values()].map((ts) => ts.length))
  return `<div class="days">${days
    .map((d) => {
      const ts = byDay.get(d)
      return `<div class="col" title="${esc(d)}: ${ts.length}">${groups.map(([label, color, test]) => `<span style="height:${(100 * ts.filter(test).length) / top}%;background:${color}" title="${esc(label)}: ${ts.filter(test).length}"></span>`).join('')}</div>`
    })
    .join('')}</div><div class="hist-labels">${days.map((d) => `<span>${esc(dayLabel(dayStart(d)))}</span>`).join('')}</div>
    <div class="legend">${groups.map(([label, color]) => `<span><i style="background:${color}"></i>${label}</span>`).join('')}</div>`
}

// ── Command evals ───────────────────────────────────────────────────────────

const RUN_HELP = `<pre>cargo run -p aloud-agent --example command_eval            # every brain with a key
cargo run -p aloud-agent --example command_eval -- gemini  # one brain
EVAL_K=3 cargo run -p aloud-agent --example command_eval   # three tries per case, for flakiness</pre>`

const matcherText = (v) =>
  typeof v === 'string' ? `contains “${v}”` : v?.eq !== undefined ? `= ${JSON.stringify(v.eq)}` : v?.or ? v.or.map((x) => matcherText(x)).join(' or ') : JSON.stringify(v)

function expectedText(c) {
  const want = c.expected?.[0]
  if (want?.not) return '<span class="muted">answer, no tool</span>'
  const name = [want?.callsTool?.name].flat().map((n) => (n.endsWith('_') ? `${n}*` : n)).join(' or ')
  const args = Object.entries(want?.callsTool?.arguments ?? {}).map(([k, v]) => `${k} ${matcherText(v)}`)
  return `<code>${esc(name)}</code>${args.length ? `<div class="caption">${esc(args.join(' · '))}</div>` : ''}`
}

const gotText = (a) =>
  !a?.called ? `<span class="muted">${a?.verdict === 'skip' ? 'not run' : 'no tool'}</span>`
  : `<code>${esc(a.called.name)}</code><div class="caption wrap">${esc(JSON.stringify(a.called.args)).slice(0, 160)}</div>`

const attemptDots = (c) => `<span class="attempts" title="${c.passes}/${c.attempts.filter((a) => a.verdict !== 'skip').length} passed">${c.attempts.map((a) => `<i class="${a.verdict === 'pass' ? '' : a.verdict}"></i>`).join('')}</span>`

const CASE_FILTERS = [
  ['all', 'All'],
  ['failing', 'Failing'],
  ['regressed', 'Regressed'],
  ['fixed', 'Fixed'],
  ['flaky', 'Flaky'],
  ['skip', 'Untested'],
]

function caseRows() {
  const f = state.caseFilter
  const query = state.caseQuery.toLowerCase()
  const rank = (r) => (r.change === 'regressed' ? 0 : r.status === 'fail' ? 1 : r.status === 'flaky' ? 2 : r.change === 'fixed' ? 3 : 4)
  return state.rows
    .filter((r) =>
      f === 'all' ? true
      : f === 'failing' ? r.status === 'fail' || r.status === 'flaky'
      : f === 'regressed' || f === 'fixed' ? r.change === f
      : r.status === f,
    )
    .filter((r) => !query || `${r.input} ${r.category} ${r.shown.called?.name ?? ''}`.toLowerCase().includes(query))
    .sort((a, b) => rank(a) - rank(b))
}

function casesTable() {
  const rows = caseRows()
  if (!rows.length) return empty('No case matches.')
  return `<div class="table-wrap"><table><thead><tr><th>Result</th><th>Said</th><th>Expected</th><th>Got</th><th>Route</th><th class="num">Time</th><th>Tries</th></tr></thead><tbody>${rows
    .map((r) => {
      const i = state.rows.indexOf(r)
      return `<tr class="click" data-case="${i}"><td>${badge(r.status, STATUS_BADGE[r.status])}${r.change ? `<div style="margin-top:4px">${badge(r.change, r.change === 'regressed' ? 'bad' : r.change === 'fixed' ? 'ok' : 'info')}</div>` : ''}</td>
        <td class="wrap said">${esc(r.input)}${r.selection ? `<div class="caption">with selection “${esc(r.selection)}”</div>` : ''}<div class="caption">${esc(r.category)}${r.failure ? ` · ${esc(r.failure)}` : ''}</div></td>
        <td>${expectedText(r)}</td><td>${gotText(r.shown)}</td><td>${esc(r.shown.route ?? '')}</td>
        <td class="num">${r.shown.route === 'brain' ? dur(r.shown.ms) : '—'}</td><td>${attemptDots(r)}</td></tr>`
    })
    .join('')}</tbody></table></div>`
}

function evals(m) {
  if (!m.runs.length) return panel('No eval runs yet', 'The command eval saves each brain’s run to the data folder, and this page reads it from there.', RUN_HELP)
  const run = m.runs.find((r) => r.file === state.run) ?? m.runs[0]
  const base = state.base === undefined ? baselineFor(run) : (m.runs.find((r) => r.file === state.base) ?? null)
  state.rows = compare(run, base)
  const regressed = state.rows.filter((r) => r.change === 'regressed').length
  const fixed = state.rows.filter((r) => r.change === 'fixed').length
  const [lo, hi] = wilson(run.pass, run.scored)
  const brainCases = run.cases.flatMap((c) => c.attempts.filter((a) => a.route === 'brain' && a.tools != null))
  const label = (r) => `${r.brain} · ${when(r.at)} · ${gitLabel(r)} · ${r.pass}/${r.scored}`
  const cats = tally(run.cases.filter((c) => c.status !== 'skip'), (c) => c.category).map(([cat]) => {
    const cs = run.cases.filter((c) => c.category === cat && c.status !== 'skip')
    const ok = cs.filter((c) => c.status === 'pass').length
    return { label: cat, values: [(100 * ok) / cs.length, (100 * (cs.length - ok)) / cs.length], note: `${ok}/${cs.length}` }
  })
  const failing = run.cases.filter((c) => c.failure)
  return `
    <div class="context-row">
      <label>Run <select class="select" id="runSelect">${m.runs.map((r) => `<option value="${esc(r.file)}" ${r === run ? 'selected' : ''}>${esc(label(r))}</option>`).join('')}</select></label>
      <label>Compared with <select class="select" id="baseSelect">
        <option value="__auto" ${state.base === undefined ? 'selected' : ''}>previous ${esc(run.brain)} run${baselineFor(run) ? '' : ' (none)'}</option>
        <option value="" ${state.base === '' ? 'selected' : ''}>nothing</option>
        ${m.runs.filter((r) => r !== run).map((r) => `<option value="${esc(r.file)}" ${state.base === r.file ? 'selected' : ''}>${esc(label(r))}</option>`).join('')}
      </select></label>
      <span>${esc(run.model)} · ${run.k} ${run.k === 1 ? 'try' : 'tries'} per case</span>
    </div>
    <div class="mini-stats">
      ${mini(`${run.pass}/${run.scored}`, `passed · 95% CI ${pct(lo, 1)}–${pct(hi, 1)}`)}
      ${mini(run.fail, 'failed every try')}
      ${run.k > 1 ? mini(run.flaky, 'flaky (passed some tries)') : ''}
      ${mini(run.skip, 'untested here')}
      ${base ? mini(`${regressed} / ${fixed}`, `regressed / fixed vs ${esc(gitLabel(base))}`) : ''}
      ${mini(dur(q(run.brainMs, 0.5)), `median brain turn · p95 ${dur(q(run.brainMs, 0.95))}`)}
      ${mini(num(q(brainCases.map((a) => a.schemaTokens), 0.5)), `schema tokens per turn · ${num(q(brainCases.map((a) => a.tools), 0.5))} tools`)}
    </div>
    <div class="grid two">
      ${panel('Where routing fails', 'Cases by what they test. Green passed every try.', cats.length ? bars(cats, { max: 100, colors: ['var(--chart-3)', 'var(--bad)'] }) : empty('Every case was untested.'))}
      ${panel('Why cases fail', 'The harness’s reason, grouped by what you would fix.', failing.length ? bars(tally(failing, (c) => c.failure).map(([k, n]) => ({ label: k, values: n, note: n })), { colors: ['var(--peach)'] }) : empty('Nothing failed in this run.'))}
    </div>
    <section class="panel section">
      <div class="panel-head"><div><h3>Every case</h3><p class="sub">Regressions first. Open a case for the arguments the brain sent.</p></div><input class="search" id="caseSearch" type="search" placeholder="Filter cases" value="${esc(state.caseQuery)}"></div>
      <div class="filters">${CASE_FILTERS.filter(([id]) => id !== "flaky" || run.k > 1).map(([id, label]) => `<button class="filter" data-case-filter="${id}" aria-pressed="${state.caseFilter === id}">${label}</button>`).join('')}</div>
      <div id="cases">${casesTable()}</div>
    </section>
    <div class="grid two section">
      ${panel('All runs', 'Newest first. Pick one to read it.', `<div class="table-wrap"><table><thead><tr><th>When</th><th>Brain</th><th>Code</th><th class="num">Passed</th><th class="num">Median turn</th></tr></thead><tbody>${m.runs
        .map((r) => `<tr class="click" data-run="${esc(r.file)}"><td>${esc(when(r.at))}</td><td>${esc(r.brain)}</td><td><code>${esc(gitLabel(r))}</code></td><td class="num">${r.pass}/${r.scored}${r.flaky ? ` · ${r.flaky} flaky` : ''}</td><td class="num">${dur(q(r.brainMs, 0.5))}</td></tr>`)
        .join('')}</tbody></table></div>`)}
      ${panel('Run it again', 'Each brain’s run is saved to the data folder, shared by every worktree, so a branch lines up against main.', RUN_HELP)}
    </div>`
}

// ── Real commands ───────────────────────────────────────────────────────────

const CMD_FILTERS = [
  ['all', 'All', () => true],
  ['failed', 'Errored', (t) => t.outcome === 'failed'],
  ['tool', 'A tool failed', (t) => t.tools.some((x) => !x.ok)],
  ['rounds', '3+ model rounds', (t) => t.rounds >= 3],
  ['asked', 'Asked you', (t) => t.asked],
  ['slow', 'Slowest 10%', (t, p90) => t.machineMs >= p90],
]

function commands(m) {
  const cs = m.commands
  if (!cs.length) return empty('No command has been traced yet.')
  const health = toolHealth(m)
  const routes = tally(cs, (t) => t.route)
  const brain = cs.filter((t) => t.route === 'brain')
  const p90 = q(cs.map((t) => t.machineMs), 0.9)
  const [, , test] = CMD_FILTERS.find(([id]) => id === state.cmdFilter)
  const shown = cs.filter((t) => test(t, p90))
  return `
    <div class="mini-stats">
      ${mini(cs.length, 'commands traced')}
      ${mini(pct(cs.filter((t) => t.outcome !== 'failed').length, cs.length), `finished · ${cs.filter((t) => t.outcome === 'failed').length} errored`)}
      ${mini(cs.filter((t) => t.outcome === 'tool failed').length, 'finished after a tool failed')}
      ${mini(pct(routes[0][1], cs.length), `went ${esc(routes[0][0])} · ${routes.slice(1).map(([r, n]) => `${esc(r)} ${pct(n, cs.length)}`).join(" · ")}`)}
      ${mini(q(brain.map((t) => t.rounds), 0.5) ?? '—', `median model rounds · p95 ${q(brain.map((t) => t.rounds), 0.95) ?? '—'}`)}
      ${mini(dur(q(cs.map((t) => t.machineMs), 0.5)), `typical time, without waits on you · p95 ${dur(q(cs.map((t) => t.machineMs), 0.95))}`)}
    </div>
    <div class="grid main-aside">
      ${panel(
        'Which tools let you down',
        'Every tool call in real use. “No case” means the eval never checks the brain picks it.',
        health.length
          ? `<div class="table-wrap"><table><thead><tr><th>Tool</th><th class="num">Calls</th><th class="num">Failed</th><th class="num">Typical</th><th>Eval</th></tr></thead><tbody>${health
              .map(
                (t) => `<tr><td><code>${esc(t.name)}</code><div class="caption">${esc(t.kit)}</div></td><td class="num">${t.calls}</td><td class="num">${t.failed ? `<span class="level-bad">${t.failed} · ${pct(t.failed, t.calls)}</span>` : '0'}</td><td class="num">${dur(q(t.ms, 0.5))}</td><td>${m.covered(t.name) ? badge('covered', 'ok') : badge('no case', 'warn')}</td></tr>`,
              )
              .join('')}</tbody></table></div>`
          : empty('No tool has run yet.'),
      )}
      ${panel(
        'How many model rounds',
        'Per command the brain handled. Each round is a model call; one is the floor.',
        hist([0, 1, 2, 3, 4, 5].map((n) => ({ label: n === 5 ? '5+' : String(n), n: brain.filter((t) => (n === 5 ? t.rounds >= 5 : t.rounds === n)).length, color: n >= 3 ? 'var(--peach)' : '' }))),
      )}
    </div>
    <section class="panel section">
      <div class="panel-head"><div><h3>Every command</h3><p class="sub">Newest first. What was said is not stored in the traces, by design; open one for its trace.</p></div></div>
      <div class="filters">${CMD_FILTERS.map(([id, label]) => `<button class="filter" data-cmd-filter="${id}" aria-pressed="${state.cmdFilter === id}">${label} <span class="muted">${cs.filter((t) => CMD_FILTERS.find(([i]) => i === id)[2](t, p90)).length}</span></button>`).join('')}</div>
      ${traceTable(shown.slice(0, 200), true)}
      ${shown.length > 200 ? `<p class="caption">Showing 200 of ${shown.length}.</p>` : ''}
    </section>`
}

function traceTable(ts, command) {
  if (!ts.length) return empty('Nothing here.')
  return `<div class="table-wrap"><table><thead><tr><th>When</th>${command ? '<th>Route</th><th>Steps</th><th class="num">Rounds</th>' : '<th>Text</th>'}<th class="num">Time</th><th>Outcome</th></tr></thead><tbody>${ts
    .map(
      (t) => `<tr class="click" data-trace="${t.id}"><td>${esc(when(t.at))}</td>${
        command
          ? `<td>${esc(t.route)}</td><td>${toolChips(t.tools)}${t.browser.length ? `<div class="caption wrap">${esc(t.browser[0].goal)}</div>` : ''}</td><td class="num">${t.rounds || ''}</td>`
          : `<td class="wrap">${t.hist ? esc(t.hist.final.slice(0, 120)) : '<span class="muted">—</span>'}</td>`
      }<td class="num">${dur(command ? t.machineMs : t.ms)}${command && t.waiting ? `<div class="caption">+${dur(t.waiting)} on you</div>` : ''}</td><td>${badge(t.outcome)}</td></tr>`,
    )
    .join('')}</tbody></table></div>`
}

// ── Browser tasks ───────────────────────────────────────────────────────────

function browser(m) {
  const ts = [...m.tasks].sort((a, b) => b.endAt - a.endAt)
  if (!ts.length) return empty('The browser engine has not run a task yet.')
  const done = ts.filter((t) => t.status === 'done')
  const statuses = tally(ts, (t) => t.status)
  const colors = { done: 'var(--chart-3)', stuck: 'var(--peach)', error: 'var(--bad)' }
  const ops = tally(ts.flatMap((t) => t.steps), (op) => op)
  return `
    <div class="mini-stats">
      ${mini(ts.length, 'browser tasks')}
      ${mini(pct(done.length, ts.length), `done · ${statuses.filter(([s]) => s !== 'done').map(([s, n]) => `${n} ${s}`).join(', ')}`)}
      ${mini(q(done.map((t) => t.steps.length), 0.5) ?? '—', 'median steps when done')}
      ${mini(dur(q(done.map((t) => t.ms).filter((v) => v != null), 0.5)), `typical time when done · p95 ${dur(q(done.map((t) => t.ms).filter((v) => v != null), 0.95))}`)}
    </div>
    <div class="grid two">
      ${panel('How browser tasks end', 'From the engine’s own log.', `<div class="stack">${statuses.map(([s, n]) => `<span style="width:${(100 * n) / ts.length}%;background:${colors[s] ?? 'var(--border-strong)'}" title="${esc(s)}: ${n}"></span>`).join('')}</div><div class="legend">${statuses.map(([s, n]) => `<span><i style="background:${colors[s] ?? 'var(--border-strong)'}"></i>${esc(s)} ${n}</span>`).join('')}</div>`)}
      ${panel('Steps before it gave up', 'Actions per stuck or failed task. Many steps means it looped; few means it never found a target.', hist(bucket([...ts.filter((t) => t.status !== 'done').map((t) => t.steps.length)], [0, 1, 2, 3, 5, 8, 12], counts).map((b) => ({ ...b, color: 'var(--peach)' }))) + `<p class="caption" style="margin-top:8px">Most used steps: ${ops.slice(0, 5).map(([op, n]) => `${esc(op)} ${n}`).join(', ')}</p>`)}
    </div>
    <section class="panel section"><div class="panel-head"><div><h3>Every browser task</h3><p class="sub">The goal is the engine’s, written from what you said.</p></div></div>
      <div class="table-wrap"><table><thead><tr><th>When</th><th>Goal</th><th>App</th><th class="num">Steps</th><th class="num">Time</th><th>Status</th></tr></thead><tbody>${ts
        .slice(0, 200)
        .map((t, i) => `<tr class="click" data-task="${i}"><td>${esc(when(t.endAt))}</td><td class="wrap">${esc(t.goal)}</td><td>${esc(t.app ?? '')}</td><td class="num">${t.steps.length}</td><td class="num">${dur(t.ms)}</td><td>${badge(t.status, t.status === 'done' ? 'ok' : t.status === 'stuck' ? 'warn' : 'bad')}</td></tr>`)
        .join('')}</tbody></table></div></section>`
}

// ── Dictation ───────────────────────────────────────────────────────────────

function dictation(m) {
  const d = m.dictations
  if (!d.length) return empty('No dictation has been traced yet.')
  const pasted = d.filter((t) => t.outcome === 'pasted')
  const outcomes = tally(d, (t) => t.outcome)
  const colors = { pasted: 'var(--chart-3)', nospeech: 'var(--border-strong)', error: 'var(--bad)', cancelled: 'var(--peach)', unrecorded: 'var(--border)' }
  const stageRows = DICTATION_STAGES.map((s) => {
    const v = pasted.map((t) => t.stage(s)).filter((x) => x != null)
    return { label: `${s} (${v.length})`, values: [q(v, 0.5) ?? 0, Math.max(0, (q(v, 0.95) ?? 0) - (q(v, 0.5) ?? 0))], note: `${dur(q(v, 0.5))} · ${dur(q(v, 0.95))}` }
  })
  const engines = tally(pasted, (t) => t.engine ?? 'unknown').map(([engine]) => {
    const v = pasted.filter((t) => (t.engine ?? 'unknown') === engine)
    return { label: `${engine} (${v.length})`, values: [q(v.map((t) => t.ms), 0.5)], note: `${dur(q(v.map((t) => t.ms), 0.5))} · p95 ${dur(q(v.map((t) => t.ms), 0.95))}` }
  })
  const lengths = [0, 2000, 5000, 15000, 60000].map((lo, i, all) => {
    const hi = all[i + 1] ?? Infinity
    const v = pasted.filter((t) => t.spokenMs != null && t.spokenMs >= lo && t.spokenMs < hi).map((t) => t.ms)
    return `<tr><td>${hi === Infinity ? `${lo / 1000} s+` : `${lo / 1000}–${hi / 1000} s`}</td><td class="num">${v.length}</td><td class="num">${dur(q(v, 0.5))}</td><td class="num">${dur(q(v, 0.95))}</td></tr>`
  })
  const silent = d.filter((t) => t.outcome === 'nospeech' && t.spokenMs != null).map((t) => t.spokenMs)
  const errors = d.filter((t) => t.outcome === "error")
  const clean = cleanupRows(m)
  const changed = clean.filter((r) => r.diff && r.diff.edits)
  return `
    <div class="mini-stats">
      ${mini(d.length, 'dictations traced')}
      ${mini(pct(pasted.length, d.length), 'ended in a paste')}
      ${mini(pct(d.filter((t) => t.outcome === 'nospeech').length, d.length), 'heard no speech')}
      ${mini(d.filter((t) => t.outcome === 'error').length, 'errors')}
      ${mini(dur(q(pasted.map((t) => t.ms), 0.5)), `release to paste · p95 ${dur(q(pasted.map((t) => t.ms), 0.95))}`)}
      ${mini(clean.length ? pct(sum(clean.filter((r) => r.diff).map((r) => r.diff.rate)), clean.filter((r) => r.diff).length) : '—', `words cleanup changed, of ${clean.length} kept in history`)}
    </div>
    <div class="grid two">
      ${panel('How dictations end', 'Every release of the key.', `<div class="stack">${outcomes.map(([o, n]) => `<span style="width:${(100 * n) / d.length}%;background:${colors[o] ?? 'var(--chart-1)'}" title="${esc(o)}: ${n}"></span>`).join('')}</div><div class="legend">${outcomes.map(([o, n]) => `<span><i style="background:${colors[o] ?? 'var(--chart-1)'}"></i>${esc(o)} ${n}</span>`).join('')}</div>`)}
      ${panel('Where the wait goes', 'Pasted dictations, per stage: typical (solid) and the 95th percentile (light).', bars(stageRows, { colors: ['var(--chart-1)', 'var(--blue)'] }))}
      ${panel('Longer speech, longer wait?', 'Release to paste, by how long the key was held.', `<table><thead><tr><th>Held</th><th class="num">Pastes</th><th class="num">Typical</th><th class="num">p95</th></tr></thead><tbody>${lengths.join('')}</tbody></table>`)}
      ${panel('Transcriber', 'Release to paste by transcription engine.', engines.length ? bars(engines, { colors: ['var(--chart-2)'] }) : empty('No engine recorded.'))}
      ${panel('Held the key, got nothing', 'Releases that heard no speech, by how long the key was held. Short ones are taps; long ones are worth a listen.', hist(bucket(silent, [0, 300, 1000, 2000, 5000, 15000], secs).map((b, i) => ({ ...b, color: i >= 3 ? 'var(--peach)' : '' }))))}
      ${panel(
        'Errors',
        'Dictations that raised an error, newest first.',
        errors.length
          ? `<div class="table-wrap"><table><thead><tr><th>When</th><th class="num">Key held</th><th>Last stage</th><th class="num">Time</th></tr></thead><tbody>${errors
              .slice(0, 12)
              .map((t) => `<tr class="click" data-trace="${t.id}"><td>${esc(when(t.at))}</td><td class="num">${dur(t.spokenMs)}</td><td><code>${esc(t.spans.filter((s) => s.stage !== 'utterance').at(-1)?.stage ?? 'none')}</code></td><td class="num">${dur(t.ms)}</td></tr>`)
              .join('')}</tbody></table></div>`
          : empty('No errors.'),
      )}
    </div>
    <section class="panel section"><div class="panel-head"><div><h3>What cleanup changed</h3><p class="sub">The last ${clean.length} dictations the history keeps, flagged first. Struck words were heard, green ones were added; punctuation and capitals are not counted.</p></div></div>
      ${clean.length ? `<div class="table-wrap"><table><thead><tr><th>When</th><th>Raw → pasted</th><th class="num">Changed</th><th>App</th></tr></thead><tbody>${[...clean]
        .sort((a, b) => b.flags.length - a.flags.length || (b.diff?.rate ?? 0) - (a.diff?.rate ?? 0))
        .map((r) => `<tr${r.utteranceId != null ? ` class="click" data-trace="${r.utteranceId}"` : ''}><td>${esc(when(r.createdAt * 1000))}</td><td class="diff wrap">${r.flags.map((f) => badge(f, 'bad')).join(' ')} ${diffHtml(r.diff)}</td><td class="num">${r.diff ? pct(r.diff.rate, 1) : '—'}</td><td>${esc(r.app ?? '')}</td></tr>`)
        .join('')}</tbody></table></div><p class="caption" style="margin-top:8px">${clean.length - changed.length} of ${clean.length} were pasted word for word.</p>` : empty('The history keeps no dictations.')}
    </section>`
}

// ── Speed ───────────────────────────────────────────────────────────────────

const SPEED_KINDS = [
  ['dictation', 'Dictation, release to paste', (m) => m.dictations.filter((t) => t.outcome === 'pasted'), (t) => t.ms],
  ['command', 'Command, without waits on you', (m) => m.commands, (t) => t.machineMs],
  ['rewrite', 'Rewrite a selection', (m) => m.traces.filter((t) => t.kind === 'rewrite'), (t) => t.ms],
]

function speed(m) {
  const rows = SPEED_KINDS.map(([id, label, pick, value]) => {
    const v = pick(m).map(value)
    return `<tr><td>${esc(label)}</td><td class="num">${v.length}</td>${[0.5, 0.9, 0.95, 0.99].map((p) => `<td class="num">${dur(q(v, p))}</td>`).join('')}<td class="num">${dur(v.length ? Math.max(...v) : null)}</td></tr>`
  })
  const stages = tally(m.traces.flatMap((t) => t.spans).filter((s) => s.stage !== 'utterance'), (s) => s.stage).map(([stage]) => {
    const ss = m.traces.flatMap((t) => t.spans).filter((s) => s.stage === stage)
    const v = ss.map((s) => s.ms)
    return `<tr><td><code>${esc(stage)}</code>${WAITS.has(stage) ? ' <span class="caption">waiting on you</span>' : ''}</td><td class="num">${v.length}</td><td class="num">${dur(q(v, 0.5))}</td><td class="num">${dur(q(v, 0.95))}</td><td class="num">${dur(Math.max(...v))}</td><td class="num">${ss.filter((s) => !s.ok).length || ''}</td></tr>`
  })
  const [, label, pick, value] = SPEED_KINDS.find(([id]) => id === state.speedKind)
  const picked = pick(m)
  const slowest = [...picked].sort((a, b) => value(b) - value(a)).slice(0, 10)
  return `
    ${panel('End to end', 'Percentiles, never averages: the slow tail is what you notice.', `<div class="table-wrap"><table><thead><tr><th>What</th><th class="num">Count</th><th class="num">p50</th><th class="num">p90</th><th class="num">p95</th><th class="num">p99</th><th class="num">Slowest</th></tr></thead><tbody>${rows.join('')}</tbody></table></div>`)}
    <div class="grid two section">
      ${panel(
        'How the times spread',
        label,
        hist(bucket(picked.map(value), [0, 500, 1000, 2000, 4000, 8000, 16000], secs)),
        `<select class="select" id="speedKind">${SPEED_KINDS.map(([id, l]) => `<option value="${id}" ${state.speedKind === id ? 'selected' : ''}>${esc(l)}</option>`).join('')}</select>`,
      )}
      ${panel('The slowest ten', `${label}. Open one for its waterfall.`, traceTable(slowest, state.speedKind === 'command'))}
    </div>
    <div class="section">${panel('Every stage', 'Each traced span, across all modes.', `<div class="table-wrap"><table><thead><tr><th>Stage</th><th class="num">Count</th><th class="num">p50</th><th class="num">p95</th><th class="num">Slowest</th><th class="num">Failed</th></tr></thead><tbody>${stages.join('')}</tbody></table></div>`)}</div>`
}

// ── Tokens & cache ──────────────────────────────────────────────────────────

function tokens(m) {
  const rounds = modelRounds(m)
  if (!rounds.length) return empty('No model round has been traced yet.')
  const usages = rounds.map((s) => s.fields.usage).filter(Boolean)
  const perCommand = m.commands.filter((t) => t.tokens.prompt)
  const models = tally(rounds, (s) => s.fields.model ?? '?').map(([model, n]) => {
    const rs = rounds.filter((s) => (s.fields.model ?? '?') === model)
    const us = rs.map((s) => s.fields.usage).filter(Boolean)
    const seen = rs.map((s) => s.at)
    return `<tr><td><code>${esc(model)}</code><div class="caption">${esc(dayLabel(Math.min(...seen)))} – ${esc(dayLabel(Math.max(...seen)))}</div></td><td class="num">${n} · ${pct(n, rounds.length)}</td><td class="num">${dur(q(rs.map((s) => s.ms), 0.5))}</td><td class="num">${dur(q(rs.map((s) => s.ms), 0.95))}</td><td class="num">${num(q(us.map((u) => u.prompt_tokens ?? 0), 0.5))}</td><td class="num">${cacheHit(us) == null ? '—' : pct(cacheHit(us), 1)}</td></tr>`
  })
  const days = [...new Set(perCommand.map((t) => dayKey(t.at)))].sort()
  const daily = days.map((d) => {
    const ts = perCommand.filter((t) => dayKey(t.at) === d)
    return { x: dayStart(d), hit: cacheHit(ts.flatMap((t) => t.usage)), tokens: q(ts.map((t) => t.tokens.prompt + t.tokens.completion), 0.5), n: ts.length }
  })
  const tokenMax = Math.max(1000, ...daily.map((d) => d.tokens))
  const other = ['verify', 'cleanup', 'transform', 'tool_discovery'].map((stage) => {
    const ss = m.traces.flatMap((t) => t.spans).filter((s) => s.stage === stage)
    return ss.length ? `<tr><td><code>${stage}</code></td><td>${esc([...new Set(ss.map((s) => s.fields.model).filter(Boolean))].join(', ') || '—')}</td><td class="num">${ss.length}</td><td class="num">${dur(q(ss.map((s) => s.ms), 0.5))}</td><td class="num">${dur(q(ss.map((s) => s.ms), 0.95))}</td></tr>` : ''
  })
  const newest = latestPerBrain(m.runs)[0]
  const evalSchema = newest?.cases.flatMap((c) => c.attempts).map((a) => a.schemaTokens).filter((v) => v != null) ?? []
  return `
    <div class="mini-stats">
      ${mini(rounds.length, 'brain rounds traced')}
      ${mini(cacheHit(usages) == null ? '—' : pct(cacheHit(usages), 1), 'of prompt tokens served from cache')}
      ${mini(num(q(usages.map((u) => u.prompt_tokens ?? 0), 0.5)), `prompt tokens per round · p95 ${num(q(usages.map((u) => u.prompt_tokens ?? 0), 0.95))}`)}
      ${mini(num(q(usages.map((u) => u.completion_tokens ?? 0), 0.5)), 'completion tokens per round')}
      ${mini(num(q(perCommand.map((t) => t.tokens.prompt + t.tokens.completion), 0.5)), `tokens per command · p95 ${num(q(perCommand.map((t) => t.tokens.prompt + t.tokens.completion), 0.95))}`)}
      ${mini(evalSchema.length ? num(q(evalSchema, 0.5)) : '—', 'tool schema tokens per turn (latest eval)')}
    </div>
    <div class="grid two">
      ${panel('Cache hits by day', 'Share of prompt tokens the provider served from cache.', lineChart([{ name: 'cache hit', color: 'var(--chart-3)', points: daily.filter((d) => d.hit != null).map((d) => ({ x: d.x, y: 100 * d.hit, title: `${dayLabel(d.x)} · ${pct(d.hit, 1)} · ${d.n} commands` })) }]))}
      ${panel('Tokens per command by day', 'Median prompt + completion tokens.', lineChart([{ name: 'tokens per command', color: 'var(--chart-1)', points: daily.filter((d) => d.tokens != null).map((d) => ({ x: d.x, y: d.tokens, title: `${dayLabel(d.x)} · ${num(d.tokens)} tokens · ${d.n} commands` })) }], { yMin: 0, yMax: tokenMax, fmt: num }))}
    </div>
    <div class="grid two section">
      ${panel('Which brain answered', 'Every brain round, with the days each model answered. Cache hit is per model: providers cache differently.', `<div class="table-wrap"><table><thead><tr><th>Model</th><th class="num">Rounds</th><th class="num">p50</th><th class="num">p95</th><th class="num">Prompt</th><th class="num">Cached</th></tr></thead><tbody>${models.join('')}</tbody></table></div>`)}
      ${panel('Other model and tool calls', 'Checks and rewrites around the brain.', `<div class="table-wrap"><table><thead><tr><th>Stage</th><th>Model</th><th class="num">Count</th><th class="num">p50</th><th class="num">p95</th></tr></thead><tbody>${other.join('')}</tbody></table></div>`)}
    </div>
    <div class="section">${panel('Tools offered per command', 'How many tool schemas the brain is handed. Every one is prompt tokens on every round.', hist(bucket(m.commands.map((t) => t.toolCount).filter((v) => v != null), [0, 10, 20, 30, 40, 60], counts)))}</div>`
}

// ── Drawers ─────────────────────────────────────────────────────────────────

function openDrawer(eyebrow, html) {
  $('drawerEyebrow').textContent = eyebrow
  $('drawerBody').innerHTML = html
  if (!$('drawer').open) $('drawer').showModal()
}

const facts = (pairs) => `<dl class="facts">${pairs.filter(([, v]) => v != null && v !== '').map(([k, v]) => `<dt>${esc(k)}</dt><dd>${v}</dd>`).join('')}</dl>`

function traceDrawer(t) {
  const kind = t.kind === 'command' ? 'Command' : t.kind === 'dictation' ? 'Dictation' : 'Rewrite'
  const firstTool = t.tools.find((x) => x.ok) ?? t.tools[0]
  const template = JSON.stringify({ input: t.browser[0]?.goal ?? '<what you said>', fast: t.route === 'fast', expected: [firstTool ? { callsTool: { name: firstTool.name } } : { not: { callsTool: {} } }] })
  openDrawer(
    `${kind} · utterance ${t.id}`,
    `<h2>${t.hist ? esc(t.hist.final || '(empty)') : t.kind === 'command' ? toolChips(t.tools) : esc(t.outcome)}</h2>
    ${facts([
      ['When', esc(when(t.at))],
      ['Outcome', badge(t.outcome)],
      ['Total', dur(t.ms)],
      ['Waiting on you', t.waiting ? dur(t.waiting) : null],
      ['Key held', t.spokenMs != null ? dur(t.spokenMs) : null],
      ['Route', t.kind === 'command' ? esc(t.route) : null],
      ['Model rounds', t.rounds || null],
      ['Tools offered', t.toolCount],
      ['Tokens', t.tokens.prompt ? `${num(t.tokens.prompt)} prompt (${num(t.tokens.cached)} cached) · ${num(t.tokens.completion)} completion` : null],
      ['Models', t.models.length ? esc([...new Set(t.models)].join(', ')) : null],
      ['Transcriber', t.engine ? esc(t.engine) : null],
    ])}
    <h3 style="margin:16px 0 8px">Waterfall</h3>${waterfall(t)}
    ${t.hist ? `<h3 style="margin:20px 0 8px">Heard → pasted</h3><p class="diff">${diffHtml(diffWords(t.hist.raw, t.hist.final))}</p>` : ''}
    ${t.browser.map((b) => `<h3 style="margin:20px 0 8px">Browser task ${badge(b.status, b.status === 'done' ? 'ok' : 'warn')}</h3><p>${esc(b.goal)}</p><p class="caption">${esc(b.steps.join(' → '))}</p>`).join('')}
    ${t.kind === 'command' ? `<h3 style="margin:20px 0 8px">Make it an eval case</h3><p class="sub">Fill in what you said, check the tool, and add the line to <code>crates/agent/evals/commands.jsonl</code>.</p><pre>${esc(template)}</pre><button class="button" data-copy="${esc(template)}" style="margin-top:8px">Copy line</button>` : ''}
    <h3 style="margin:20px 0 8px">Spans</h3><pre>${esc(JSON.stringify(t.spans.map(({ stage, ms, ok, fields }) => ({ stage, ms, ok, ...fields })), null, 1))}</pre>`,
  )
}

function caseDrawer(c) {
  openDrawer(
    `Eval case · ${c.category}`,
    `<h2>${esc(c.input)}</h2>
    ${facts([
      ['Result', badge(c.status, STATUS_BADGE[c.status])],
      ['Before', c.was ? badge(c.was, STATUS_BADGE[c.was]) : null],
      ['Change', c.change ? esc(c.change) : null],
      ['Selection', c.selection ? esc(c.selection) : null],
      ['Fast path', c.fast == null ? null : c.fast ? 'expected to take it' : 'expected to skip it'],
      ['Why it failed', c.failure ? esc(c.failure) : null],
    ])}
    <h3 style="margin:16px 0 8px">Expected</h3><pre>${esc(JSON.stringify(c.expected, null, 2))}</pre>
    ${c.attempts
      .map(
        (a, i) => `<h3 style="margin:20px 0 8px">Try ${i + 1} ${badge(a.verdict, STATUS_BADGE[a.verdict])}</h3>
      ${facts([
        ['Why', a.why ? esc(a.why) : null],
        ['Route', esc(a.route)],
        ['Time', a.route === 'brain' ? dur(a.ms) : null],
        ['Rate-limited', a.rateLimitedMs ? dur(a.rateLimitedMs) : null],
        ['Tools offered', a.tools != null ? `${a.tools} (~${num(a.schemaTokens)} schema tokens)` : null],
      ])}<pre>${esc(JSON.stringify(a.called, null, 2) ?? 'no tool call')}</pre>`,
      )
      .join('')}`,
  )
}

function taskDrawer(t) {
  openDrawer(
    `Browser task · ${t.id}`,
    `<h2>${esc(t.goal)}</h2>
    ${facts([
      ['Status', badge(t.status, t.status === 'done' ? 'ok' : 'warn')],
      ['Ended', esc(when(t.endAt))],
      ['Time', dur(t.ms)],
      ['Browser', esc(t.app ?? '')],
    ])}
    <h3 style="margin:16px 0 8px">Steps</h3><p class="caption">${esc(t.steps.join(' → ') || 'none')}</p>
    ${t.answer ? `<h3 style="margin:16px 0 8px">What it read back</h3><pre>${esc(t.answer.join('\n'))}</pre>` : ''}`,
  )
}

const DEFINITIONS = [
  ['Eval case', 'One line of crates/agent/evals/commands.jsonl: something said, and the tool (and argument matchers) the fast path or brain must pick. Only the first turn is checked; nothing runs.'],
  ['Passed / failed / flaky', 'With EVAL_K above 1 a brain case is asked several times. Passed means every try passed (pass^k); flaky means some did.'],
  ['Untested', 'The expected tool was not offered on this machine or account when the eval ran, so the case says nothing either way.'],
  ['95% CI', 'Wilson score interval on the pass rate. With a few dozen cases it is wide on purpose.'],
  ['Regressed / fixed', 'Compared with the baseline run, matched by what was said. Regressed: passed before, fails now.'],
  ['Route', 'fast: the no-model path ran a Mac tool. web: the fast path sent it straight to the browser engine. brain: the tool-calling model. Older traces are inferred from their spans.'],
  ['Model round', 'One tool_select call to the brain. One is the floor; each extra round is another model call.'],
  ['Waiting on you', 'confirm and ask_user spans: time the run spent waiting for your answer. Command times leave it out.'],
  ['Release to paste', 'The dictation’s utterance span, which opens on the key’s release edge and closes after the paste.'],
  ['Cache hit', 'cached_tokens over prompt_tokens, from the provider’s usage block on each brain round.'],
  ['Cleanup changed', 'Word-level edits between what the transcriber heard and what was pasted, ignoring punctuation and capitals.'],
  ['Where the data lives', 'Eval runs: <data>/evals/*.json. Traces: timings.sqlite. Text: history.sqlite, only as many dictations as History keeps. Browser tasks: engine/log.jsonl. All local; this page is served on 127.0.0.1 only.'],
]

// ── Wiring ──────────────────────────────────────────────────────────────────

document.addEventListener('click', async (e) => {
  const el = e.target.closest('[data-tab],[data-action],[data-trace],[data-case],[data-task],[data-run],[data-case-filter],[data-cmd-filter],[data-copy]')
  if (!el) return
  const d = el.dataset
  if (d.tab) {
    state.tab = d.tab
    history.replaceState(null, '', `#${d.tab}`)
    $('drawer').close()
    render()
    window.scrollTo({ top: document.querySelector('.tabs')?.offsetTop - 16 })
  } else if (d.action === 'refresh') load()
  else if (d.action === 'close') $('drawer').close()
  else if (d.action === 'definitions') openDrawer('Definitions', `<h2>What each number means</h2><dl class="defs">${DEFINITIONS.map(([k, v]) => `<dt>${esc(k)}</dt><dd>${esc(v)}</dd>`).join('')}</dl>`)
  else if (d.trace) traceDrawer(state.m.traces.find((t) => t.id === Number(d.trace)))
  else if (d.case) caseDrawer(state.rows[Number(d.case)])
  else if (d.task) taskDrawer([...state.m.tasks].sort((a, b) => b.endAt - a.endAt)[Number(d.task)])
  else if (d.run) {
    state.run = d.run
    state.base = undefined
    render()
  } else if (d.caseFilter) {
    state.caseFilter = d.caseFilter
    document.querySelectorAll('[data-case-filter]').forEach((b) => b.setAttribute('aria-pressed', b.dataset.caseFilter === state.caseFilter))
    $('cases').innerHTML = casesTable()
  } else if (d.cmdFilter) {
    state.cmdFilter = d.cmdFilter
    render()
  } else if (d.copy) {
    await navigator.clipboard.writeText(d.copy)
    el.textContent = 'Copied'
  }
})

document.addEventListener('change', (e) => {
  if (e.target.id === 'runSelect') {
    state.run = e.target.value
    state.base = undefined
  } else if (e.target.id === 'baseSelect') state.base = e.target.value === '__auto' ? undefined : e.target.value
  else if (e.target.id === 'speedKind') state.speedKind = e.target.value
  else return
  render()
})

document.addEventListener('input', (e) => {
  if (e.target.id !== 'caseSearch') return
  state.caseQuery = e.target.value
  $('cases').innerHTML = casesTable()
})

$('drawer').addEventListener('click', (e) => {
  if (e.target === $('drawer')) $('drawer').close() // the backdrop
})

async function load() {
  $('freshness').textContent = 'Loading…'
  try {
    const raw = await (await fetch('/api/data')).json()
    state.m = build(raw)
    $('dataDir').textContent = raw.dataDir
    $('freshness').textContent = `${plural(state.m.traces.length, 'trace')} · ${plural(state.m.runs.length, 'run')} · read ${new Date(raw.generatedAt).toLocaleTimeString()}`
    render()
  } catch (err) {
    $('freshness').textContent = 'Could not read the data'
    $('main').innerHTML = `<div class="notice">Could not load /api/data: ${esc(err.message)}. Is <code>pnpm evals</code> running?</div>`
  }
}

load()
