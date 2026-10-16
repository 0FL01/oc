#!/usr/bin/env bun
// VIS41 development oracle: pinned source bodies, never an application capture.
import fs from "node:fs"
import path from "node:path"
import assert from "node:assert/strict"
import { createHash } from "node:crypto"
import { spawnSync } from "node:child_process"

const repo = path.resolve(import.meta.dir, "../..")
const pin = "2670273ff17da96f85c5826ced57aa1b368754fa"
const sha = (data: string | Buffer) => createHash("sha256").update(data).digest("hex")
const output = path.resolve(process.argv[2] || "")
if (process.argv[3] === "phases") {
  const manifest = JSON.parse(fs.readFileSync(path.join(output, "source-manifest.json"), "utf8"))
  for (const [file, hash] of Object.entries(manifest.executable_sha256))
    assert.equal(sha(fs.readFileSync(path.join(output, "executable", file))), hash)
  const marquee = await import(path.join(output, "executable", "marquee.ts"))
  const title = process.argv[4], width = Number(process.argv[5])
  assert(title && width > 0 && width <= 160)
  console.log(JSON.stringify({ title, width, cycle_width: marquee.marqueeCycleWidth(title),
    overflow: marquee.marqueeOverflows(title, width), phases: Array.from({ length: marquee.marqueeCycleWidth(title) }, (_, offset) =>
      ({ offset, parts: marquee.marqueeTextParts(title, width, offset), text: marquee.marqueeText(title, width, offset) })) }))
  process.exit(0)
}
assert.equal(path.dirname(output), path.join(repo, "evidence/tui/recovery-v00"))
assert.match(path.basename(output), /^tabs-oracle[a-z0-9-]+$/)
fs.mkdirSync(output)
fs.mkdirSync(path.join(output, "sources"))
fs.mkdirSync(path.join(output, "executable"))
const sources = JSON.parse(fs.readFileSync(path.join(repo, "tui-recovery/SOURCES.json"), "utf8"))
const entries = (Array.isArray(sources) ? sources : Object.values(sources).find(Array.isArray)) as { id: string; path: string; commit: string }[]
const selected = entries.filter(s => ["U46", "U47", "U48", "U49", "U56", "U57", "U58", "U59"].includes(s.id))
assert.equal(selected.length, 8)
const extra = ["packages/tui/src/util/locale.ts", "packages/tui/src/util/string-width.bun.ts",
  "packages/tui/src/context/session-tabs-model.ts", "packages/tui/src/component/spinner-frames.ts", "packages/tui/src/ui/layout.ts", "LICENSE"]
const texts: Record<string, string> = {}, hashes: Record<string, string> = {}
for (const file of [...new Set([...selected.map(s => s.path), ...extra])]) {
  const original = spawnSync("git", ["show", pin + ":" + file], { cwd: path.join(repo, "opencode"), encoding: "utf8" })
  assert.equal(original.status, 0, original.stderr)
  assert.equal(fs.readFileSync(path.join(repo, "opencode", file), "utf8"), original.stdout, "Dirty pinned source " + file)
  texts[file] = original.stdout
  const name = file.replaceAll("/", "__")
  fs.writeFileSync(path.join(output, "sources", name), original.stdout, { flag: "wx" })
  hashes[file] = sha(original.stdout)
}
const executable: Record<string, string> = {}
const normalize = (name: string, code: string) => {
  fs.writeFileSync(path.join(output, "executable", name), code, { flag: "wx" })
  executable[name] = sha(code)
}
normalize("string-width.ts", texts[extra[1]])
normalize("locale.ts", texts[extra[0]])
normalize("marquee.ts", texts[selected.find(s => s.id === "U57")!.path])
normalize("session-tabs-model.ts", texts[extra[2]])
const original = texts[selected.find(s => s.id === "U56")!.path]
const controllers = original.slice(original.indexOf("export function createMarquee("), original.indexOf("\nfunction TabContextMenu("))
assert(controllers.startsWith("export function createMarquee(") && controllers.endsWith("\n"))
// Only the host is substituted: off-mode tween jumps, Solid signals/cleanup,
// and fake timers. The two extracted controller bodies are byte-identical.
normalize("controllers.ts", `import { marqueeOverflows, marqueeCycleWidth } from './marquee'
import { createSignal, onCleanup, createAnimatable, tween, setTimeout, setInterval, clearTimeout, clearInterval } from './host'
const MARQUEE_DELAY = 600, MARQUEE_INTERVAL = 80
${controllers}`)
normalize("host.ts", `let now = 0, serial = 1
const jobs = new Map<number, { at: number; callback: () => void; repeat?: number }>()
const cleanups: (() => void)[] = []
export function createSignal<T>(initial?: T) { let value = initial; return [() => value, (next: T | ((value: T) => T)) => value = typeof next === 'function' ? (next as (value: T) => T)(value as T) : next] as const }
export function onCleanup(fn: () => void) { cleanups.push(fn) }
export const tween = (options: unknown) => options
export function createAnimatable<T>(initial: T, options: { enabled: () => boolean }) { if (options.enabled()) throw Error('Oracle only substitutes off-mode animation host'); let value = initial; return { value: () => value, jump: (next: T) => value = next, animate: (next: T) => value = next } }
export function setTimeout(callback: () => void, ms = 0) { const id = serial++; jobs.set(id, { at: now + ms, callback }); return id }
export function setInterval(callback: () => void, ms: number) { const id = serial++; jobs.set(id, { at: now + ms, callback, repeat: ms }); return id }
export const clearTimeout = (id: number) => jobs.delete(id), clearInterval = clearTimeout
export function advance(ms: number) { const end = now + ms; while (true) { const next = [...jobs].filter(([, job]) => job.at <= end).sort((a, b) => a[1].at - b[1].at || a[0] - b[0])[0]; if (!next) break; now = next[1].at; if (next[1].repeat) next[1].at += next[1].repeat; else jobs.delete(next[0]); next[1].callback() } now = end }
export function pending() { return jobs.size }
export function dispose() { for (const fn of cleanups.splice(0)) fn(); jobs.clear(); now = 0 }
`)
const m = await import(path.join(output, "executable/marquee.ts"))
const c = await import(path.join(output, "executable/controllers.ts"))
const host = await import(path.join(output, "executable/host.ts"))
const layout = await import(path.join(output, "executable/session-tabs-model.ts"))
const checks: string[] = []
const check = (name: string, fn: () => void) => { fn(); checks.push(name); host.dispose() }
check("U59 short/exact-hover-fit", () => { assert.equal(m.marqueeText("Short", 10, 8), "Short"); assert(!m.marqueeOverflows("Exact fit", 9)); assert(m.marqueeOverflows("Exact fit", 8)) })
check("U59 source offsets/one spaced-dot cycle", () => { for (const [offset, text] of [[0, "A long s"], [2, "long ses"], [15, "title · "], [20, " · A lon"]] as const) assert.equal(m.marqueeText("A long session title", 8, offset), text); assert.equal(m.marqueeText("A long session title", 8, m.marqueeCycleWidth("A long session title")), "A long s") })
check("U59 generated dot versus title dot", () => { assert.deepEqual(m.marqueeTextParts("A · title", 6, 7), [{ value: "l", separator: false }, { value: "e", separator: false }, { value: " ", separator: false }, { value: "·", separator: true }, { value: " ", separator: false }, { value: "A", separator: false }]); assert.deepEqual(m.marqueeTextParts("A · title", 6, 2)[0], { value: "·", separator: false }) })
check("U59 Unicode display cells", () => assert.equal(m.marqueeText("Plan 🧭 the release", 8, 5), "🧭 the r"))
check("U58 delay/short-next with animations:false", () => { const state = c.createMarquee(() => false); state.enter("first", "opencode", 6); host.advance(599); assert.equal(state.offset(), 0); host.advance(1); assert.equal(state.offset(), 1); state.enter("second", "short", 6); assert.equal(state.active(), undefined); assert.equal(host.pending(), 0) })
check("U58 one-cycle retains identity/same-tab motion does not restart", () => { const state = c.createMarquee(() => false); state.enter("first", "opencode", 6); host.advance(1400); assert.equal(state.offset(), 0); assert.equal(state.active(), "first"); assert.equal(state.leading(), 0); assert.equal(host.pending(), 0); state.enter("first", "opencode", 6); host.advance(2000); assert.equal(host.pending(), 0) })
check("U58 ordinary leave/reset/disposal", () => { const state = c.createMarquee(() => false); state.enter("first", "opencode", 6); host.advance(700); state.leave("first"); assert.equal(state.active(), undefined); assert.equal(state.offset(), 0); assert.equal(host.pending(), 0); state.enter("second", "opencode", 6); host.dispose(); assert.equal(host.pending(), 0) })
check("U56 deferred nested leave then reentry / U58 rail leave", () => { const state = c.createTabMarquee(() => false); state.enter("first", "opencode", 6); host.advance(700); const offset = state.offset(); state.leave("first"); state.enter("first", "opencode", 6); host.advance(0); assert.equal(state.offset(), offset); state.leaveHovered(); host.advance(0); assert.equal(state.hovered(), undefined); assert.equal(state.active(), undefined); assert.equal(host.pending(), 0) })
check("U56 compact Infinity disables title overflow", () => assert(!m.marqueeOverflows("A very long title", Infinity)))
const profiles = [[80, 24], [120, 40], [160, 48]].map(([columns, rows]) => {
  const tabs = [{ sessionID: "root-a" }, { sessionID: "root-b" }]
  const widths = layout.adaptiveSessionTabLayout(tabs, "root-a", columns - 3).widths
  return { columns, rows, two_root_tab_widths: widths, horizontal_resting: widths.map((w: number) => w - 3), horizontal_hovered: widths.map((w: number) => w - 5), vertical_noncompact: { rail_width: 42, resting: 38, hovered: 37 }, compact: { hovered_width: "Infinity", no_title_marquee: true } }
})
fs.writeFileSync(path.join(output, "source-manifest.json"), JSON.stringify({ commit: pin, sources: selected, source_sha256: hashes, executable_sha256: executable, controller_body_sha256: sha(controllers), controller_body_changed: false, host: "Deterministic signals/cleanup/timers, off-mode jump only; no renderer or enabled tween substitution claim" }, null, 2) + "\n", { flag: "wx" })
fs.writeFileSync(path.join(output, "result.json"), JSON.stringify({ status: "PASS_PINNED_SOURCE_ORACLE", checks, profiles, source_ids: selected.map(s => s.id), qualification: "Source/controller fixture oracle only; actual running original/native and enabled visual tween are separate gates" }, null, 2) + "\n", { flag: "wx" })
fs.writeFileSync(path.join(output, "NOTICE.md"), `# VIS39 own-running / VIS41 source oracle\n\nPinned anomalyco/opencode v2.0.12, ${pin}, MIT; complete license and byte-verified source are in sources/. Pure marquee/locale/layout modules retain their bodies; createMarquee/createTabMarquee bodies are extracted byte-for-byte with an explicit deterministic off-mode test host. No source renderer, JS product runtime, application clock replacement, or screenshot-derived input. U46–U49/U56–U59 and amendment lines 882–962 govern qualification.\n`, { flag: "wx" })
console.log(JSON.stringify({ output, checks: checks.length, status: "PASS_PINNED_SOURCE_ORACLE" }))
