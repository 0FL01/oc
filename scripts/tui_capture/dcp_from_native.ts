// SPDX-License-Identifier: AGPL-3.0-or-later
// Reference-only adapter: qualified actual owner -> unchanged pinned D05.
// This exports reference payloads; it does not import anything into native oc.
import fs from "node:fs"
import path from "node:path"
import assert from "node:assert/strict"
import { createHash } from "node:crypto"
import { fileURLToPath } from "node:url"

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..")
const parent = path.join(repo, "evidence/tui/recovery-v00")
const [oracleArg, nativeArg, outputArg] = process.argv.slice(2)
assert(oracleArg && nativeArg && outputArg, "PINNED_ORACLE_DIR QUALIFIED_NATIVE_DIR NEW_ORACLE_DIR")
const oracle = path.resolve(oracleArg), native = path.resolve(nativeArg), output = path.resolve(outputArg)
for (const dir of [oracle, native, output]) assert.equal(path.dirname(dir), parent)
assert.match(path.basename(output), /^dcp-oracle[a-z0-9-]+$/)
const sha = (data: string | Buffer) => createHash("sha256").update(data).digest("hex")
const read = (file: string) => JSON.parse(fs.readFileSync(file, "utf8"))
const qualification = read(path.join(native, "result.json"))
assert.equal(qualification.status, "PASS_ACTUAL_SINGLE_MULTI_RESTART_OWNER_PROBE", "Only the actual-owner probe qualifies; display fixture JSON is not ingress")
assert.equal(qualification.parent_released_sha256, qualification.oc_binary_sha256)
assert.match(qualification.oc_binary_sha256, /^[a-f0-9]{64}$/)
for (const [name, hash] of Object.entries(qualification.evidence_seals)) {
  assert.equal(path.basename(name), name)
  assert.equal(sha(fs.readFileSync(path.join(native, name))), hash, name + " owner evidence seal")
}
const nativeRows = (file: string, table: string) => read(path.join(native, file)).flatMap((o: any) => o.data[table] ?? [])
const result = read(path.join(oracle, "result.json")), manifest = read(path.join(oracle, "source-manifest.json"))
assert.equal(result.status, "PASS_PINNED_DISPLAY_ORACLE")
assert.equal(result.goldens_sha256, sha(fs.readFileSync(path.join(oracle, "goldens.json"))))
assert.equal(manifest.dcp_pin, "11f6517780a502512a3467645074be447cb0369e")
assert.equal(manifest.oc_pin, "2670273ff17da96f85c5826ced57aa1b368754fa")
const hashes: Record<string, string> = {
  D05: "626a8978c1386ea206ae1600642a0c268e00f6db85b30670049dd2d33ffb319b",
  D06: "3a7f92b604906530d7494511d07330db3c2b6d8ad695c8cea3bca2b819fec7cb",
  D10: "47e1d1693698f31be16371b7172658d0b446469be0608ed2c1aceabf98e5eaf2",
}
const stripImports = (code: string) => code.replace(/^import\s+(?:type\s+)?[\s\S]*?\s+from\s+["'][^"']+["']\s*;?\r?\n/gm, "")
for (const source of manifest.sources) {
  assert(source.local.startsWith("sources/") && !source.local.includes(".."))
  assert.equal(sha(fs.readFileSync(path.join(oracle, source.local))), source.sha256)
  if (hashes[source.id]) assert.equal(source.sha256, hashes[source.id])
}
for (const [id, file] of [["D05", "notification.ts"], ["D06", "utils.ts"], ["D10", "state-utils.ts"]]) {
  const source = manifest.sources.find((s: any) => s.id === id)
  assert.equal(stripImports(fs.readFileSync(path.join(oracle, source.local), "utf8")), stripImports(fs.readFileSync(path.join(oracle, "executable", file), "utf8")), id + " unchanged body")
  assert.equal(sha(fs.readFileSync(path.join(oracle, "executable", file))), read(path.join(oracle, `imports-${id}.json`)).executable_sha256)
}
fs.mkdirSync(output) // validation precedes new immutable attempt
const save = (file: string, data: unknown) => fs.writeFileSync(path.join(output, file), JSON.stringify(data, null, 2) + "\n", { flag: "wx" })
fs.cpSync(path.join(oracle, "sources"), path.join(output, "sources"), { recursive: true, errorOnExist: true, force: false })
fs.cpSync(path.join(oracle, "executable"), path.join(output, "executable"), { recursive: true, errorOnExist: true, force: false })
for (const file of ["NOTICE.md", "imports-D05.json", "imports-D06.json", "imports-D10.json"]) fs.copyFileSync(path.join(oracle, file), path.join(output, file), fs.constants.COPYFILE_EXCL)
save("source-manifest.json", { ...manifest, runner_sha256: sha(fs.readFileSync(fileURLToPath(import.meta.url))), inherited_source_oracle: oracle })
const { sendCompressNotification } = await import(path.join(output, "executable/notification.ts"))
const { formatProgressBar } = await import(path.join(output, "executable/utils.ts"))
const { getActiveSummaryTokenUsage, serializePruneMessagesState } = await import(path.join(output, "executable/state-utils.ts"))
const notifications = []
const replayInputs: any[] = []
for (const check of qualification.checks.filter((c: any) => ["single", "multi", "recompression"].includes(c.stage))) {
  const run = check.typed_snapshot
  const stored = nativeRows(`compress-${check.stage}-snapshot.json`, "dcp_run_views").find((r: any) => r.operation_id === run.operation_id)
  assert(stored && JSON.stringify(JSON.parse(stored.snapshot)) === JSON.stringify(run), "Typed snapshot must be actual stored operation data")
  assert.equal(run.removed, check.independent_removed)
  assert.equal(run.net_saved, check.independent_net_saved)
  assert.equal(run.new_messages, check.new_message_ids.length)
  assert.equal(run.new_tools, check.new_tool_ids.length)
  assert.equal(check.new_tool_ids.length, 0, "This qualified disjoint probe has no covered calls; do not fabricate occurrence identities")
  const storedBlocks = nativeRows(`compress-${check.stage}-snapshot.json`, "compression_blocks")
  const memberships = nativeRows(`compress-${check.stage}-snapshot.json`, "compression_members")
  const context = nativeRows(`compress-${check.stage}-snapshot.json`, "conversation_messages").sort((a: any, b: any) => a.seq - b.seq)
  const blocks = check.active_summaries.map((active: any, index: number) => {
    const actual = storedBlocks.find((b: any) => b.id === active.block_id)
    assert(actual)
    const current = check.range_inputs.find((r: any) => r.block_id === active.block_id)
    if (current) assert.equal(current.summary, actual.summary)
    return { blockId: index + 1, nativeBlockId: actual.id, runId: run.ordinal,
      topic: actual.topic, summary: actual.summary, summaryTokens: active.summary_tokens,
      compressedTokens: current?.removed ?? 0, active: true,
      directMessageIds: current?.new_message_ids ?? [], directToolIds: current?.new_tool_ids ?? [],
      effectiveMessageIds: memberships.filter((m: any) => m.block_id === actual.id).map((m: any) => m.message_id), effectiveToolIds: [] }
  })
  const entries = run.block_ids.map((id: string) => {
    const b = blocks.find((b: any) => b.nativeBlockId === id)
    assert(b)
    return { blockId: b.blockId, runId: run.ordinal, summary: b.summary, summaryTokens: b.summaryTokens }
  })
  assert.equal(entries.reduce((n: number, e: any) => n + e.summaryTokens, 0), run.summary)
  assert.equal(blocks.reduce((n: number, b: any) => n + b.compressedTokens, 0), run.removed)
  const pruned = new Set([...check.prior_message_ids, ...check.new_message_ids])
  const state = { stats: { totalPruneTokens: run.cumulative.gross_removed, pruneTokenCounter: 0 },
    prune: { messages: { blocksById: new Map(blocks.map((b: any) => [b.blockId, b])),
      activeBlockIds: new Set(blocks.map((b: any) => b.blockId)),
      byMessageId: new Map(check.commit_message_ids.map((id: string) => {
        const activeBlockIds = blocks.filter((b: any) => b.effectiveMessageIds.includes(id)).map((b: any) => b.blockId)
        assert.equal(activeBlockIds.length > 0, pruned.has(id))
        const actual = context.find((r: any) => r.id === id)
        assert(actual)
         return [id, { tokenCount: Math.floor((actual.text.length + 2) / 4), allBlockIds: activeBlockIds, activeBlockIds }]
      })),
      activeByAnchorMessageId: new Map(), nextBlockId: blocks.length + 1, nextRunId: run.ordinal + 1 } } }
  assert.equal(getActiveSummaryTokenUsage(state), run.cumulative.active_summary)
  assert.equal(formatProgressBar(check.commit_message_ids, new Map([...pruned].map(id => [id, 0])), check.new_message_ids, 50).slice(1, -1), run.bar)
  const config = { pruneNotification: check.display.notification, pruneNotificationType: check.display.channel, compress: { showCompression: check.display.show_compression } }
  const prompts: any[] = [], toasts: any[] = [], errors: any[] = []
  const sent = await sendCompressNotification({ session: { prompt: async (p: any) => { prompts.push(p) } }, tui: { showToast: async (p: any) => { toasts.push(p) } } }, { error: (...e: any[]) => errors.push(e) }, config, state, run.session, entries, run.topic, check.commit_message_ids, { agent: "build", providerId: "fixture", modelId: "fixture-model-1" })
  assert.equal(errors.length, 0)
  assert.deepEqual(context.slice(0, -1).map((r: any) => r.id), check.commit_message_ids)
  replayInputs.push({check, run, state, entries, context})
  notifications.push({ id: `native-${check.stage}`, result: sent, prompts, toasts, errors,
    active_summary_tokens: run.cumulative.active_summary, payload: prompts[0]?.body.parts[0].text ?? toasts[0]?.body.message ?? null,
    fixture: { config, state: { stats: state.stats, prune: { messages: serializePruneMessagesState(state.prune.messages) } } },
    native_context: { session: run.session, operation_id: run.operation_id, typed_snapshot: run, messages: context,
      prior_notifications: notifications.map((n: any) => ({ operation_id: n.native_context.operation_id,
        before_message_id: n.native_context.messages.at(-1).id, payload: n.payload })),
      capture_after: context.at(-1).text, capture_before: context[0].text,
      stage: check.stage, required_native_capture: "Capture this genuine stage context before advancing the owner campaign; later session tails are not the same full-frame fixture" } })
}
assert.equal(notifications.length, 3)
const operationNotifications = [...notifications]
for (const displayCase of qualification.display_cases.filter((c: any) => ["restart", "minimal", "off", "show-false", "toast"].includes(c.stage))) {
  assert.equal(displayCase.typed_snapshot.operation_id, replayInputs[2].run.operation_id)
  const display = displayCase.display
  if (["minimal", "off", "show-false", "toast"].includes(displayCase.stage)) {
    const actualConfig = read(path.join(native, `control-${displayCase.stage}-config.json`))
    assert.equal(actualConfig.pruneNotification, display.notification)
    assert.equal(actualConfig.pruneNotificationType, display.channel)
    assert.equal(actualConfig.compress.showCompression, display.show_compression)
  }
  const config = {pruneNotification:display.notification, pruneNotificationType:display.channel,
    compress:{showCompression:display.show_compression}}
  const rerender = async (input: any) => {
    const prompts: any[] = [], toasts: any[] = [], errors: any[] = []
    const sent = await sendCompressNotification({session:{prompt:async (p: any) => {prompts.push(p)}},tui:{showToast:async (t: any) => {toasts.push(t)}}},
      {error:(...e: any[]) => errors.push(e)},config,input.state,input.run.session,input.entries,input.run.topic,input.check.commit_message_ids,
      {agent:"build",providerId:"fixture",modelId:"fixture-model-1"})
    assert.equal(errors.length,0)
    return {result:sent,prompts,toasts,errors,payload:prompts[0]?.body.parts[0].text??null}
  }
  const current = await rerender(replayInputs[2])
  const context = nativeRows(displayCase.context_snapshot,'conversation_messages').sort((a:any,b:any)=>a.seq-b.seq)
  assert.equal(context.at(-1).role,'assistant')
  const prior_notifications = []
  for (const input of replayInputs.slice(0,2)) {
    const before_message_id = input.context.at(-1).id
    assert(context.some((r:any)=>r.id===before_message_id))
    prior_notifications.push({operation_id:input.run.operation_id,before_message_id,payload:(await rerender(input)).payload})
  }
  const before_message_id = replayInputs[2].context.at(-1).id
  assert(context.some((r:any)=>r.id===before_message_id))
  prior_notifications.push({operation_id:replayInputs[2].run.operation_id,before_message_id,payload:current.payload})
  // Current operation card already belongs to committed history. For a resumed
  // capture its imported prior card is inserted at its original boundary; the
  // final assistant is the genuine restart continuation, not a fourth card.
  notifications.push({id:`native-${displayCase.stage}`, ...current,
    active_summary_tokens:replayInputs[2].run.cumulative.active_summary,
    fixture:{config, state:{stats:replayInputs[2].state.stats,
      prune:{messages:serializePruneMessagesState(replayInputs[2].state.prune.messages)}}},
    native_context:{session:replayInputs[2].run.session,operation_id:replayInputs[2].run.operation_id,
      typed_snapshot:replayInputs[2].run,messages:context,prior_notifications,
      capture_after:context.at(-1).text,capture_before:context[0].text,
      stage:displayCase.stage,display,reference_delivery:'Historical operation at original boundary; no new compression',
      required_native_capture:displayCase.output}})
}
const owner = { path: native, result_sha256: sha(fs.readFileSync(path.join(native, "result.json"))),
  binary_sha256: qualification.oc_binary_sha256, source_HEAD: qualification.source_HEAD, evidence_seals: qualification.evidence_seals }
save("owner-association.json", owner)
save("goldens.json", { schema_version: 1, provenance: { package: "@tarquinen/opencode-dcp", version: "3.1.15", commit: manifest.dcp_pin,
  license: "AGPL-3.0-or-later", native_owner: owner, qualification: "Actual qualified native accounting -> unchanged pinned D05 reference payload. Native PTY pairing remains separate; no native JSON ingress." }, notifications, bars: [], tokens: [] })
save("result.json", { status: "PASS_PINNED_DISPLAY_ORACLE", qualification: "Qualified native-derived reference payload, not paired TUI PASS",
  actual_native_compress: qualification.status, goldens_sha256: sha(fs.readFileSync(path.join(output, "goldens.json"))), owner,
  body_changes: false, provider_requests: 0, native_capture_gate: "Genuine stage-context native styled cells / PNG / cursor are still required" })
console.log(JSON.stringify({ output, status: "PASS_PINNED_DISPLAY_ORACLE", native_derived_notifications: notifications.length, committed_operations: operationNotifications.length }))
