// Renders every tab against a small fixture under Node, and checks the
// figures that can be wrong while still rendering. No browser, no packages.
//
//   node evals/check.mjs

import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import vm from 'node:vm'

const T0 = Date.parse('2026-10-01T10:00:00Z')
const span = (stage, at, ms, fields = {}, ok = true) => ({ stage, at: T0 + at, ms, ok, fields })
const usage = (prompt, cached) => ({ prompt_tokens: prompt, completion_tokens: 40, prompt_tokens_details: { cached_tokens: cached } })
const run = (brain, startedAt, verdicts) => ({
  file: `${startedAt}-${brain}.json`,
  brain,
  model: brain === 'cerebras' ? 'qwen' : 'flash-lite',
  k: 1,
  startedAt,
  git: { sha: 'abc1234', branch: 'main', dirty: false },
  cases: [
    { input: 'open notes and add milk', expected: [{ callsTool: { name: 'ALOUD_NOTES_APPEND' } }], attempts: [verdicts[0]] },
    { input: 'remind me at 5', expected: [{ callsTool: { name: 'ALOUD_REMINDERS_ADD', arguments: { due: '17:00' } } }], attempts: [verdicts[1]] },
    { input: 'post in slack', expected: [{ callsTool: { name: 'SLACK_' } }], attempts: [{ verdict: 'skip', why: '"SLACK_" not offered here', route: 'brain' }] },
  ],
})
const pass = { verdict: 'pass', route: 'brain', ms: 400, called: { name: 'X', args: {} } }

const fixture = {
  generatedAt: T0,
  dataDir: '/fixture',
  evalSet: [{ input: 'x', expected: [{ callsTool: { name: 'ALOUD_NOTES_APPEND' } }] }],
  evalRuns: [
    run('cerebras', '2026-10-02T10:00:00Z', [
      { verdict: 'fail', why: 'called ALOUD_OPEN_APP {"names":["Notes"]}', route: 'brain', ms: 500, called: { name: 'ALOUD_OPEN_APP', args: {} } },
      { verdict: 'fail', why: 'ALOUD_REMINDERS_ADD `due` = "05:00"', route: 'brain', ms: 450, called: { name: 'ALOUD_REMINDERS_ADD', args: { due: '05:00' } } },
    ]),
    run('cerebras', '2026-10-01T10:00:00Z', [pass, { ...pass, verdict: 'fail', why: 'answered without calling a tool' }]),
  ],
  traces: [
    { id: 4, spans: [span('utterance', 9000, 3000, { mode: 'agent' }), span('agent_run', 9100, 2800, { tool_count: 20, route: 'brain' }),
      span('tool_select', 9200, 300, { model: 'qwen', round: 0, usage: usage(6000, 5000) }), span('ALOUD_MUSIC', 9500, 100, {}),
      span('tool_exec', 9500, 100, { tool: 'ALOUD_MUSIC', toolkit: 'macOS', ok: false }, false),
      span('tool_select', 9600, 300, { model: 'flash-lite', round: 1, usage: usage(6000, 0) }), span('confirm', 9900, 1000, {}),
      span('tool_exec', 10900, 100, { tool: 'ALOUD_SPOTIFY', toolkit: 'macOS', ok: true })] },
    { id: 3, spans: [span('utterance', 6000, 20, { mode: 'dictation', outcome: 'error', spoken_ms: 3 }, false)] },
    { id: 2, spans: [span('utterance', 3000, 50, { mode: 'dictation', outcome: 'nospeech', spoken_ms: 2500 })] },
    { id: 1, spans: [span('utterance', 0, 900, { mode: 'dictation', outcome: 'pasted', spoken_ms: 1800 }), span('transcribe', 50, 600, { engine: 'Cloud' }),
      span('cleanup', 650, 200, { model: 'gpt-oss' }), span('paste', 850, 50, {})] },
  ],
  history: [
    { id: 2, createdAt: T0 / 1000 + 5, raw: ' .', final: ' .', app: null, durationMs: 400, utteranceId: null },
    { id: 1, createdAt: T0 / 1000, raw: 'um hello world', final: 'Hello, world.', app: 'Notes', durationMs: 1800, utteranceId: 1 },
  ],
  browserTasks: [
    { id: 'a', goal: 'open youtube', app: 'Brave', status: 'stuck', endAt: T0 + 20000, ms: 5000, steps: ['CLICK', 'CLICK'], answer: null },
    { id: 'b', goal: 'go to github', app: 'Brave', status: 'done', endAt: T0 + 30000, ms: 2000, steps: ['OPEN_URL'], answer: ['ok'] },
  ],
  stats: null,
}

// Just enough DOM for app.js: every element it touches is a plain object.
const elements = {}
const element = () => ({ textContent: '', innerHTML: '', open: false, close() {}, showModal() { this.open = true }, addEventListener() {}, setAttribute() {} })
const context = vm.createContext({
  document: { getElementById: (id) => (elements[id] ??= element()), querySelector: () => null, querySelectorAll: () => [], addEventListener() {} },
  location: { hash: '' },
  history: { replaceState() {} },
  window: { scrollTo() {} },
  navigator: {},
  fetch: async () => ({ json: async () => fixture }),
  console,
})
vm.runInContext(readFileSync(join(import.meta.dirname, 'app.js'), 'utf8'), context)
await new Promise((resolve) => setTimeout(resolve, 0)) // let load() resolve
const run$ = (code) => vm.runInContext(code, context)
// Values cross back as JSON: arrays from the page's realm fail deepStrictEqual otherwise.
const val = (code) => JSON.parse(vm.runInContext(`JSON.stringify(${code})`, context))

// Percentiles and intervals.
assert.equal(val('q([4, 1, 3, 2], 0.5)'), 2)
assert.equal(val('q([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 0.95)'), 10)
assert.equal(val('q([], 0.5)'), null)
assert.deepEqual(val('wilson(0, 0)'), [0, 1])
assert.ok(val('wilson(10, 10)[0]') > 0.7 && val('wilson(10, 10)[0]') < 0.75)

// The harness's reasons, grouped by what to fix.
assert.equal(val('failureKind("called ALOUD_OPEN_APP {}")'), 'Wrong tool')
assert.equal(val('failureKind("ALOUD_REMINDERS_ADD `due` = \\"05:00\\"")'), 'Right tool, wrong arguments')
assert.equal(val('failureKind("ALOUD_NOTES_APPEND without `title`: {}")'), 'Right tool, wrong arguments')
assert.equal(val('failureKind("fast path missed")'), 'Fast path misroute')

// A pass that now fails is a regression; a fail that still fails is not; a skip is neither.
const rows = val('compare(state.m.runs[0], baselineFor(state.m.runs[0])).map((r) => [r.status, r.change])')
assert.deepEqual(rows, [['fail', 'regressed'], ['fail', null], ['skip', null]])

// Cleanup diff ignores punctuation and capitals; a lone "." is flagged.
assert.equal(val('diffWords("um hello world", "Hello, world.").edits'), 1)
assert.deepEqual(val('cleanupRows(state.m).map((r) => r.flags)'), [['pasted only punctuation'], []])

// Command time leaves out the wait on the user; a failed tool that recovered still finished.
const command = val('state.m.commands[0]')
assert.equal(command.machineMs, 2000)
assert.equal(command.outcome, 'tool failed')
assert.equal(command.tokens.cached, 5000)

// Findings name the real problems.
const titles = val('findings(state.m).map((f) => f.title)').join('\n')
for (const want of ['regressed on cerebras', 'pasted something suspicious', 'press too short', 'failed over', 'browser tasks got stuck'])
  assert.ok(titles.includes(want), `missing finding “${want}” in:\n${titles}`)

// Every tab and drawer renders.
for (const [tab] of val('TABS')) {
  run$(`state.tab = ${JSON.stringify(tab)}; render()`)
  assert.ok(elements.main.innerHTML.length > 500, `${tab} rendered almost nothing`)
}
run$('traceDrawer(state.m.traces[0]); caseDrawer(state.rows[0]); taskDrawer(state.m.tasks[0])')
assert.ok(elements.drawerBody.innerHTML.includes('open youtube'))

console.log('evals dashboard: all checks passed')
