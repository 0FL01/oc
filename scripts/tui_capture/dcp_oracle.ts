// SPDX-License-Identifier: AGPL-3.0-or-later
// Reference-only VIS38 oracle. Executes pinned DCP modules, never native Rust.
// Source bodies remain byte-identical; only imports are replaced in executable copies.
import fs from "node:fs"
import path from "node:path"
import assert from "node:assert/strict"
import { createHash } from "node:crypto"
import { fileURLToPath } from "node:url"

const pin = "11f6517780a502512a3467645074be447cb0369e"
const ocPin = "2670273ff17da96f85c5826ced57aa1b368754fa"
const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..")
const output = path.resolve(process.argv[2] || "")
const parent = path.join(repo, "evidence/tui/recovery-v00")
assert.equal(path.dirname(output), parent, "Use a new direct child of evidence/tui/recovery-v00")
assert.match(path.basename(output), /^dcp-oracle[a-z0-9-]+$/)
fs.mkdirSync(output) // immutable attempt; no recursive reuse
const sha = (data: string | Buffer) => createHash("sha256").update(data).digest("hex")
const save = (name: string, data: unknown) => fs.writeFileSync(path.join(output, name), JSON.stringify(data, null, 2) + "\n", { flag: "wx" })
const originals = path.join(output, "sources")
const executable = path.join(output, "executable")
fs.mkdirSync(originals)
fs.mkdirSync(executable)
const sources = [
  ["D05", "lib/ui/notification.ts"], ["D06", "lib/ui/utils.ts"],
  ["D07", "lib/compress/state.ts"], ["D08", "lib/config.ts"],
  ["D09", "lib/token-utils.ts"], ["D10", "lib/state/utils.ts"],
  ["D10-range", "lib/compress/range.ts"], ["package", "package.json"], ["license", "LICENSE"],
]
const manifest: any[] = []
const texts: Record<string, string> = {}
for (const [id, sourcePath] of sources) {
  const url = `https://raw.githubusercontent.com/Opencode-DCP/opencode-dynamic-context-pruning/${pin}/${sourcePath}`
  const response = await fetch(url)
  assert(response.ok, `${response.status} ${url}`)
  const bytes = Buffer.from(await response.arrayBuffer())
  texts[id] = bytes.toString("utf8")
  const local = path.join("sources", sourcePath.replaceAll("/", "__"))
  fs.writeFileSync(path.join(output, local), bytes, { flag: "wx" })
  manifest.push({ id, commit: pin, path: sourcePath, url, sha256: sha(bytes), local })
}
assert.equal(JSON.parse(texts.package).version, "3.1.15")
assert.equal(JSON.parse(texts.package).license, "AGPL-3.0-or-later")
const wrapperPath = "opencode/packages/tui/src/routes/session/index.tsx"
for (const [id, sourcePath] of [["U34", wrapperPath.slice(9)], ["OC-import", "packages/cli/src/commands/handlers/session/import.ts"], ["OC-transfer", "packages/schema/src/session-transfer.ts"], ["OC-message", "packages/schema/src/session-message.ts"], ["OC-license", "LICENSE"]]) {
  const url = `https://raw.githubusercontent.com/anomalyco/opencode/${ocPin}/${sourcePath}`
  const response = await fetch(url)
  assert(response.ok, `${response.status} ${url}`)
  const bytes = Buffer.from(await response.arrayBuffer())
  assert.equal(sha(bytes), sha(fs.readFileSync(path.join(repo, "opencode", sourcePath))), "Local donor must match exact network pin")
  const local = path.join("sources", "oc__" + sourcePath.replaceAll("/", "__"))
  fs.writeFileSync(path.join(output, local), bytes, { flag: "wx" })
  manifest.push({ id, commit: ocPin, path: sourcePath, url, sha256: sha(bytes), local })
}
save("source-manifest.json", { dcp_pin: pin, oc_pin: ocPin, sources: manifest, runner_sha256: sha(fs.readFileSync(fileURLToPath(import.meta.url))) })
fs.writeFileSync(path.join(output, "NOTICE.md"), `# VIS38 reference provenance\n\nPinned DCP 3.1.15, ${pin}, https://github.com/Opencode-DCP/opencode-dynamic-context-pruning, AGPL-3.0-or-later. Original source and complete license are in sources/. Executable copies change imports only; their formatter/bar/accounting bodies are unchanged. This fixture driver is reference-only; it does not install a plugin or add a production JS dependency. OC2 wrapper is MIT, ${ocPin}; its original license is in the donor opencode/LICENSE.\n`, { flag: "wx" })

// Dependencies outside the exercised display graph fail closed rather than
// substituting an estimator/query implementation. The real active-summary owner
// (D10) runs; all fixture blocks include explicit token estimates.
fs.writeFileSync(path.join(executable, "services.ts"), `const unused = () => { throw Error("Oracle unexpectedly used an unqualified external service") };\nexport const countTokens=unused, isIgnoredUserMessage=unused, messageHasCompress=unused, isMessageWithInfo=unused;\n`, { flag: "wx" })
const imports = /^import\s+(?:type\s+)?[\s\S]*?\s+from\s+["'][^"']+["']\s*;?\r?\n/gm
for (const [id, filename] of [["D05", "notification.ts"], ["D06", "utils.ts"], ["D10", "state-utils.ts"]]) {
  const changes: any[] = []
  const code = texts[id].replace(imports, declaration => {
    let replacement = ""
    if (declaration.includes('from "./utils"')) replacement = declaration.replace('"./utils"', '"./utils.ts"')
    if (declaration.includes('from "../state/utils"')) replacement = declaration.replace('"../state/utils"', '"./state-utils.ts"')
    if (declaration.includes('from "../token-utils"') || declaration.includes('from "../messages/query"') || declaration.includes('from "../messages/shape"')) {
      replacement = declaration.replace(/from\s+"[^"]+"/, 'from "./services.ts"')
    }
    changes.push({ original: declaration, replacement })
    return replacement
  })
  fs.writeFileSync(path.join(executable, filename), code, { flag: "wx" })
  save(`imports-${id}.json`, { original_sha256: sha(texts[id]), executable_sha256: sha(code), changes, body_change: false })
}
const { sendCompressNotification } = await import(path.join(executable, "notification.ts"))
const { formatTokenCount, formatProgressBar } = await import(path.join(executable, "utils.ts"))
const { getActiveSummaryTokenUsage, serializePruneMessagesState, loadPruneMessagesState } = await import(path.join(executable, "state-utils.ts"))
const ids = Array.from({ length: 10 }, (_, i) => `m${i + 1}`)
const prior = { blockId: 3, runId: 2, topic: "Earlier analysis", summary: "Keep prior requirement.", summaryTokens: 260, compressedTokens: 3000, active: true, directMessageIds: ["m1", "m2"], directToolIds: ["call-a"], effectiveMessageIds: ["m1", "m2"], effectiveToolIds: ["call-a"] }
const current = { blockId: 11, runId: 7, topic: "Formatter qualification", summary: "Preserve R1; continue without replaying tools.", summaryTokens: 842, compressedTokens: 11900, active: true, directMessageIds: ["m3", "m4", "m5"], directToolIds: ["call-b", "call-c"], effectiveMessageIds: ["m3", "m4", "m5"], effectiveToolIds: ["call-b", "call-c"] }
const stateFor = (blocks: any[], messageIds = ids) => ({
  stats: { totalPruneTokens: 14900, pruneTokenCounter: 400 },
  prune: { messages: { blocksById: new Map(blocks.map(b => [b.blockId, b])), activeBlockIds: new Set(blocks.filter(b => b.active).map(b => b.blockId)), byMessageId: new Map(messageIds.map(id => [id, { tokenCount: 100, allBlockIds: blocks.filter(b => (b.effectiveMessageIds || b.directMessageIds).includes(id)).map(b => b.blockId), activeBlockIds: blocks.filter(b => b.active && (b.effectiveMessageIds || b.directMessageIds).includes(id)).map(b => b.blockId) }])), activeByAnchorMessageId: new Map(), nextBlockId: 13, nextRunId: 8 } },
})
const entryFor = (b: any) => ({ blockId: b.blockId, runId: b.runId, summary: b.summary, summaryTokens: b.summaryTokens })
const config = { pruneNotification: "detailed", pruneNotificationType: "chat", compress: { showCompression: false } }
const base = { blocks: [prior, current], entries: [entryFor(current)], sessionMessageIds: ids, batchTopic: undefined, config }
const second = { ...current, blockId: 12, topic: "Unicode continuation: 日本語 résumé", summary: "Retain R2 and stable occurrence IDs.", summaryTokens: 170, compressedTokens: 2700, directMessageIds: ["m5", "m6"], directToolIds: ["call-c", "call-d"], effectiveMessageIds: ["m5", "m6"], effectiveToolIds: ["call-c", "call-d"] }
const zero = { ...current, summaryTokens: 0, summary: "", directToolIds: [], effectiveToolIds: [] }
const large = { ...current, summaryTokens: 1000000, compressedTokens: 4218800 }
const recompressed = { ...current, compressedTokens: 842, summaryTokens: 90, directMessageIds: [], directToolIds: [], effectiveMessageIds: ["m1", "m2", "m3"], effectiveToolIds: ["call-a", "call-b"], consumedBlockIds: [3] }
const long = { ...current, topic: "Long topic 日本語 résumé — " + "requirements and continuation ".repeat(8), summary: "## Preserved requirements\n" + Array.from({ length: 18 }, (_, i) => `- R${i + 1}: retain 日本語 and tool pairs; never replay an unknown effect.`).join("\n") }
const cases = [
  { id: "detailed", ...base },
  { id: "minimal", ...base, config: { ...config, pruneNotification: "minimal" } },
  { id: "minimal-show", ...base, config: { ...config, pruneNotification: "minimal", compress: { showCompression: true } } },
  { id: "off", ...base, config: { ...config, pruneNotification: "off" } },
  { id: "show-single", ...base, config: { ...config, compress: { showCompression: true } } },
  { id: "multi-range", ...base, blocks: [prior, current, second], entries: [entryFor(current), entryFor(second)], batchTopic: "Two independent ranges", config: { ...config, compress: { showCompression: true } } },
  { id: "zero-summary-tools", ...base, blocks: [prior, zero], entries: [entryFor(zero)] },
  { id: "empty-positions", ...base, sessionMessageIds: [] },
  { id: "recompression", ...base, blocks: [{ ...prior, active: false }, recompressed], entries: [entryFor(recompressed)] },
  { id: "large-k-only", ...base, blocks: [large], entries: [entryFor(large)], totals: { totalPruneTokens: 4218800, pruneTokenCounter: 0 } },
  { id: "long-topic-summary", ...base, blocks: [prior, long], entries: [entryFor(long)], config: { ...config, compress: { showCompression: true } } },
  { id: "toast", ...base, blocks: [prior, long], entries: [entryFor(long)], config: { ...config, pruneNotificationType: "toast", compress: { showCompression: true } } },
  { id: "empty-entries", ...base, entries: [] },
]
const goldens: any[] = []
for (const fixture of cases) {
  const state = stateFor(fixture.blocks, fixture.sessionMessageIds)
  if (fixture.totals) state.stats = fixture.totals
  const prompts: any[] = [], toasts: any[] = [], errors: any[] = []
  const result = await sendCompressNotification({ session: { prompt: async (p: any) => { prompts.push(p) } }, tui: { showToast: async (p: any) => { toasts.push(p) } } }, { error: (...args: any[]) => errors.push(args) }, fixture.config, state, "ses_dcp_oracle", fixture.entries, fixture.batchTopic, fixture.sessionMessageIds, { agent: "build", providerId: "fixture", modelId: "fixture-model-1" })
  const serialized = serializePruneMessagesState(state.prune.messages)
  const reloaded = { ...state, prune: { messages: loadPruneMessagesState(serialized) } }
  assert.equal(getActiveSummaryTokenUsage(reloaded), getActiveSummaryTokenUsage(state))
  goldens.push({ id: fixture.id, fixture: { ...fixture, state: { stats: state.stats, prune: { messages: serialized } } }, result, prompts, toasts, errors, active_summary_tokens: getActiveSummaryTokenUsage(state), payload: prompts[0]?.body.parts[0].text ?? toasts[0]?.body.message ?? null })
}
const byId = (id: string) => goldens.find(g => g.id === id)
assert.equal(byId("off").payload, null)
assert.equal(byId("empty-entries").result, false)
assert.equal(byId("minimal").payload, byId("minimal-show").payload)
assert(byId("multi-range").payload.includes("4 messages and 3 tools compressed"))
assert(byId("multi-range").payload.includes("### Unicode continuation: 日本語 résumé"))
assert(byId("zero-summary-tools").payload.includes("→ Items: 3 messages compressed"))
assert(!byId("zero-summary-tools").payload.includes("+0"))
assert(byId("recompression").payload.includes("0 messages compressed"))
assert.equal(byId("recompression").active_summary_tokens, 90)
assert.equal(byId("large-k-only").payload.split("\n")[0], "▣ DCP | -4218.8K removed, +1000K summary")
assert.equal(byId("toast").prompts.length, 0)
for (const g of goldens.filter(g => g.prompts.length)) {
  assert.equal(g.prompts[0].body.noReply, true)
  assert.equal(g.prompts[0].body.parts[0].ignored, true)
  assert.equal(g.errors.length, 0)
}
const bars = [
  { id: "bar0", messages: [], pruned: [], recent: [] },
  { id: "bar1", messages: ["a"], pruned: ["a"], recent: [] },
  { id: "recent-overrides-prior", messages: ["a", "b", "c", "d"], pruned: ["a", "b"], recent: ["b", "c"] },
  { id: "bar50", messages: Array.from({ length: 50 }, (_, i) => String(i)), pruned: ["0", "20"], recent: ["20", "49"] },
  { id: "bar101-zero-span", messages: Array.from({ length: 101 }, (_, i) => String(i)), pruned: ["0", "2", "3", "50"], recent: ["0", "50", "99", "100"] },
].map(f => ({ ...f, width: 50, payload: formatProgressBar(f.messages, new Map(f.pruned.map(id => [id, 100])), f.recent, 50) }))
assert.equal(bars[0].payload, "│" + "░".repeat(50) + "│")
assert.equal(bars[2].payload, "│" + "░".repeat(12) + "⣿".repeat(25) + "█".repeat(13) + "│")
assert.equal([...bars[4].payload].length, 52)
const nativeValues: Record<string, string> = { "0": "0", "842": "842", "999": "999", "1000": "1K", "1049": "1K", "1050": "1.1K", "1149": "1.1K", "1150": "1.2K", "11900": "11.9K", "999949": "999.9K", "999950": "1M", "1000000": "1M", "4218800": "4.2M" }
const tokens = Object.entries(nativeValues).map(([input, native]) => ({ input: Number(input), pinned_compact: formatTokenCount(Number(input), true), pinned_verbose: formatTokenCount(Number(input)), approved_native_compact: native, named_difference: formatTokenCount(Number(input), true) === native ? null : Number(input) < 999950 ? "VIS38-native-decimal-half-up-versus-JS-toFixed" : "VIS38-native-M-and-K-boundary-promotion" }))
save("goldens.json", { schema_version: 1, provenance: { package: "@tarquinen/opencode-dcp", version: "3.1.15", commit: pin, license: "AGPL-3.0-or-later", method: "Execute unchanged pinned formatter/bar/active-summary owner with import-only normalization and intercepted external prompt/toast delivery", qualification: "source-derived display; not plugin runtime compatibility or actual native compression" }, notifications: goldens, bars, tokens })
save("native-equivalent-fixtures.json", {
  schema_version: 1,
  qualification: "Synthetic typed display-unit fixtures only. NOT actual native compress outcomes and NOT permitted ingress for VIS38 paired acceptance. Paired references must be regenerated from qualified actual native accounting by dcp_from_native.ts.",
  snapshot_type: "oc_core::dcp_view::DcpRunSnapshot",
  fixtures: goldens.filter(g => g.fixture.entries.length).map(g => {
    const f = g.fixture
    const currentBlocks = f.entries.map((e: any) => f.blocks.find((b: any) => b.blockId === e.blockId))
    const newlyCovered = [...new Set(currentBlocks.flatMap((b: any) => b.directMessageIds))]
    const newlyCoveredTools = [...new Set(currentBlocks.flatMap((b: any) => b.directToolIds))]
    const removed = currentBlocks.reduce((n: number, b: any) => n + b.compressedTokens, 0)
    const summary = f.entries.reduce((n: number, e: any) => n + e.summaryTokens, 0)
    const gross = f.state.stats.totalPruneTokens + f.state.stats.pruneTokenCounter
    const active = new Map(Object.entries(f.state.prune.messages.byMessageId).filter(([, e]: any) => e.activeBlockIds.length).map(([id, e]: any) => [id, e.tokenCount]))
    return { id: g.id, display: { notification: f.config.pruneNotification, channel: f.config.pruneNotificationType, show_compression: f.config.compress.showCompression }, snapshot: {
      session: "reference-fixture-session", operation_id: `reference-display-fixture-${g.id}`, ordinal: f.entries[0].runId,
      topic: f.batchTopic ?? currentBlocks[0].topic, block_ids: currentBlocks.map((b: any) => `b${b.blockId}`),
      removed, summary, net_saved: Math.max(0, removed - summary), method: "utf16_round_quarter_fallback",
      new_messages: newlyCovered.length, new_tools: newlyCoveredTools.length,
      cumulative: { gross_removed: gross, active_summary: g.active_summary_tokens, net_saved: Math.max(0, gross - g.active_summary_tokens), compressions: f.entries[0].runId, prunes: 1, method: "utf16_round_quarter_fallback", complete: true },
      bar: formatProgressBar(f.sessionMessageIds, active, newlyCovered, 50).slice(1, -1),
    }, summaries: currentBlocks.map((b: any) => ({ block_id: `b${b.blockId}`, topic: b.topic, text: b.summary })), pinned_payload: g.payload,
    named_differences: g.id === "large-k-only" ? ["VIS38-native-M-and-K-boundary-promotion"] : g.id === "toast" ? ["VIS38-native-bounded-transient-toast-mapping"] : [] }
  }),
})
save("result.json", { status: "PASS_PINNED_DISPLAY_ORACLE", notification_cases: goldens.length, bar_cases: bars.length, token_cases: tokens.length, goldens_sha256: sha(fs.readFileSync(path.join(output, "goldens.json"))), body_changes: false, provider_requests: 0, actual_native_compress: "NOT_RUN_AWAIT_PARENT_FINAL_BINARY_RELEASE" })
console.log(JSON.stringify({ output, status: "PASS_PINNED_DISPLAY_ORACLE", notifications: goldens.length, bars: bars.length, tokens: tokens.length }))
