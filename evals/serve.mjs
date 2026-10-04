// The evals dashboard: command-eval runs saved by `command_eval`, beside what
// the app recorded in real use, read straight from Aloud's data folder.
//
//   pnpm evals            # http://127.0.0.1:8091
//   node evals/check.mjs  # renders every tab against a fixture, checks the maths
//
// Bound to loopback only: the traces and history hold what was dictated.
// ALOUD_DATA points it at another data folder, PORT at another port.

import { createServer } from 'node:http'
import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { DatabaseSync } from 'node:sqlite'

const here = import.meta.dirname
// Mirrors `aloud_notes::data_dir()`.
const dataDir =
  process.env.ALOUD_DATA ??
  (process.platform === 'darwin'
    ? join(homedir(), 'Library/Application Support/Aloud')
    : join(process.env.APPDATA ?? homedir(), 'Aloud'))
const port = Number(process.env.PORT ?? 8091)

const parse = (text) => {
  try {
    return JSON.parse(text)
  } catch {
    return null
  }
}

// Read-only, so the running app keeps writing while the page reads.
function query(file, sql) {
  const path = join(dataDir, file)
  if (!existsSync(path)) return []
  const db = new DatabaseSync(path, { readOnly: true })
  try {
    return db.prepare(sql).all()
  } finally {
    db.close()
  }
}

function traces() {
  const byId = new Map()
  const spans = query(
    'timings.sqlite',
    'SELECT utterance_id, stage, started_at, duration_ms, success, fields FROM timings ORDER BY started_at, id',
  )
  for (const row of spans) {
    const fields = parse(row.fields) ?? {}
    // The provider's usage block arrives as a JSON string inside the JSON.
    if (typeof fields.usage === 'string') fields.usage = parse(fields.usage)
    let trace = byId.get(row.utterance_id)
    if (!trace) byId.set(row.utterance_id, (trace = { id: row.utterance_id, spans: [] }))
    trace.spans.push({
      stage: row.stage,
      at: row.started_at,
      ms: row.duration_ms,
      ok: Boolean(row.success),
      fields,
    })
  }
  return [...byId.values()].sort((a, b) => b.id - a.id)
}

const history = () =>
  query(
    'history.sqlite',
    `SELECT id, created_at AS createdAt, raw_transcript AS raw, final_transcript AS final,
            app_name AS app, duration_ms AS durationMs, utterance_id AS utteranceId
     FROM history ORDER BY id DESC`,
  )

function jsonl(path) {
  if (!existsSync(path)) return []
  return readFileSync(path, 'utf8').split('\n').filter(Boolean).map(parse).filter(Boolean)
}

// One row per finished browser task; the log's `at` is when it ended.
const browserTasks = () =>
  jsonl(join(dataDir, 'engine/log.jsonl'))
    .filter((line) => line.kind === 'task')
    .map(({ at, id, goal, app, status, elapsed_ms, steps, answer }) => ({
      id,
      goal,
      app,
      status,
      endAt: Date.parse(at),
      ms: elapsed_ms ?? null,
      // Actions are upper-case (CLICK, TYPE_TEXT); the lower-case ops are the
      // engine's verdict (done, stuck), not steps it took.
      steps: (steps ?? []).map((s) => s.op).filter((op) => op && op === op.toUpperCase()),
      answer: answer?.lines?.slice(0, 6) ?? null,
    }))

function evalRuns() {
  const dir = join(dataDir, 'evals')
  if (!existsSync(dir)) return []
  return readdirSync(dir)
    .filter((name) => name.endsWith('.json'))
    .map((name) => ({ file: name, ...parse(readFileSync(join(dir, name), 'utf8')) }))
    .filter((run) => Array.isArray(run.cases))
    .sort((a, b) => b.startedAt.localeCompare(a.startedAt))
}

const data = () => ({
  generatedAt: Date.now(),
  dataDir,
  evalSet: jsonl(join(here, '../crates/agent/evals/commands.jsonl')),
  evalRuns: evalRuns(),
  traces: traces(),
  history: history(),
  browserTasks: browserTasks(),
  stats: parse(existsSync(join(dataDir, 'stats.json')) ? readFileSync(join(dataDir, 'stats.json'), 'utf8') : 'null'),
})

const files = {
  '/': ['index.html', 'text/html'],
  '/app.js': ['app.js', 'text/javascript'],
  '/styles.css': ['styles.css', 'text/css'],
}

createServer((req, res) => {
  const path = new URL(req.url, 'http://x').pathname
  try {
    if (path === '/api/data') {
      res.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' })
      return res.end(JSON.stringify(data()))
    }
    const file = files[path]
    if (!file) {
      res.writeHead(404)
      return res.end('not found')
    }
    res.writeHead(200, { 'content-type': `${file[1]}; charset=utf-8`, 'cache-control': 'no-store' })
    res.end(readFileSync(join(here, file[0])))
  } catch (e) {
    res.writeHead(500, { 'content-type': 'text/plain' })
    res.end(String(e?.stack ?? e))
  }
}).listen(port, '127.0.0.1', () => {
  console.log(`Aloud evals: http://127.0.0.1:${port}  (data: ${dataDir})`)
})
