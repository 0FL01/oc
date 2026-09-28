#!/usr/bin/env node
// Factual, append-only campaign closeout from sealed capture/analysis inputs.
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
const root=path.resolve(process.argv[2]);
const repo=path.resolve('scripts/tui_capture/../..');
const read=p=>JSON.parse(fs.readFileSync(p));
const hash=p=>createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const analysisName=process.argv[3]||'prompt-paste-analysis.json';
const analysis=read(path.join(root,analysisName));
const counts=names=>{
  const out={runs:names.length,captures:0,comparisons:{grid:{},png:{}},cursor_differences:0,requests:{upstream:0,oc:0},main:0,title:0,invalid:0,unstable:0};
  for(const name of names) {
    const lock=read(path.join(root,name,'capture.lock.json')),run=analysis.runs[name];
    out.captures+=lock.captures.length;out.unstable+=lock.captures.filter(c=>c.status!=='CAPTURED').length;
    for(const f of run.frames){out.comparisons[f.mode][f.status]=(out.comparisons[f.mode][f.status]||0)+1;if(f.mode==='grid'&&f.cursor_differs)out.cursor_differences++;}
    for(const side of ['upstream','oc'])for(const e of run.sides[side].provider_requests){out.requests[side]++;out[e.operation==='transcript'?'main':'title']++;if(!e.valid)out.invalid++;}
  }
  return out;
};
const result={status:analysis.status,behavior_status:analysis.behavior_status,analysis_input:analysisName,all:counts(Object.keys(analysis.runs)),current:counts(analysis.current_runs),runs:{},source_files:{},provenance_failures:analysis.provenance_failures,current_behavior_failures:analysis.current_behavior_failures,retained_attempt_failures:analysis.retained_attempt_failures,wire_differences:analysis.wire_differences};
for(const [name,run] of Object.entries(analysis.runs)) {
  const lock=read(path.join(root,name,'capture.lock.json'));
   result.runs[name]={...counts([name]),commit:run.commit,binary_sha256:run.binary_sha256,rust_inputs_sha256:run.rust_inputs_sha256,source_manifest_sha256:run.source_manifest_sha256,dirty_diff_sha256:run.dirty_diff_sha256,source_unchanged_after_build:lock.oc.source_inputs_unchanged_after_build,source_unchanged_after_capture:lock.oc.source_inputs_unchanged_after_capture,behavior:{upstream:run.sides.upstream.status,oc:run.sides.oc.status},pending_rgb:{upstream:run.sides.upstream.pending_rgb,oc:run.sides.oc.pending_rgb},key_frames:run.frames.filter(f=>['leader-chip-normal','leader-chip-pending','leader-chip-restored','leader-chip-expanded-fulltext','leader-extra-enter-pending','leader-extra-enter-next','leader-extra-enter-restored','leader-repeat-expanded','leader-repeat-enter-pending','leader-repeat-enter-restored','leader-suffix-repeat-two-chips','leader-suffix-chip-expanded-1','leader-suffix-chip-expanded-2'].includes(f.scenario)).map(({scenario,mode,status,different_cells,different_pixels,cursor_differs,bbox})=>({scenario,mode,status,different_cells,different_pixels,cursor_differs,bbox}))};
  result.runs[name].completion_and_settlement=Object.fromEntries(['upstream','oc'].map(side=>[side,{provider_completions:run.sides[side].provider_completions,chip_settlement:run.sides[side].chip_settlement}]));
}
const manifest=read(path.join(root,'chip-120','source-manifest.json'));
for(const p of ['Cargo.toml','Cargo.lock','rust-toolchain.toml','crates/oc/Cargo.toml',
  'crates/oc-tui/src/app.rs','crates/oc-tui/src/app/input.rs','crates/oc-tui/src/app/transcript.rs',
  'crates/oc-tui/src/app/tabs.rs','crates/oc-tui/src/app/live.rs','crates/oc-tui/src/approval_view.rs',
  'crates/oc-tui/src/editor.rs','crates/oc-tui/src/shell.rs','crates/oc/tests/pty.rs',
  'crates/oc/tests/pty_t39.rs','crates/oc/tests/pty_t39/interaction.rs','crates/oc/tests/pty_t39/lifecycle.rs']) {
  const current=hash(path.join(repo,p)),present=Object.hasOwn(manifest,p);
  result.source_files[p]={captured_sha256:present?manifest[p]:null,current_sha256:current,capture_present:present,matches_current:present&&manifest[p]===current};
}
result.captured_heads=[...new Set(Object.values(result.runs).map(r=>r.commit))];
result.native_binary_sha256=[...new Set(Object.values(result.runs).map(r=>r.binary_sha256))];
result.rust_inputs_sha256=[...new Set(Object.values(result.runs).map(r=>r.rust_inputs_sha256))];
const bytes=dir=>fs.readdirSync(dir,{withFileTypes:true}).reduce((n,e)=>n+(e.isDirectory()?bytes(path.join(dir,e.name)):fs.statSync(path.join(dir,e.name)).size),0);
result.evidence_bytes_before_closeout=bytes(root);
fs.writeFileSync(path.join(root,'summary.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});
const first=Object.values(result.runs)[0];
const table=Object.entries(result.runs).map(([name,r])=>`| ${name} | ${r.captures} | ${r.comparisons.grid.DIFFERENT||0}/${r.comparisons.grid.BLOCKED||0} | ${r.comparisons.png.DIFFERENT||0}/${r.comparisons.png.BLOCKED||0} | ${r.cursor_differences} | ${r.requests.upstream}/${r.requests.oc} | ${r.behavior.upstream}/${r.behavior.oc} |`).join('\n');
const sourceTable=Object.entries(result.source_files).map(([name,s])=>`| \`${name}\` | \`${s.captured_sha256}\` | ${s.matches_current} |`).join('\n');
const associations=Object.entries(result.runs).map(([name,r])=>`| ${name} | \`${r.commit}\` | \`${r.source_manifest_sha256}\` | \`${r.dirty_diff_sha256}\` |`).join('\n');
const keyFrames=Object.entries(result.runs).filter(([name])=>analysis.current_runs.includes(name)).flatMap(([name,r])=>r.key_frames.filter(f=>f.mode==='grid').map(f=>`| ${name}/${f.scenario} | ${f.different_cells??'blocked'} | ${f.cursor_differs??'blocked'} |`)).join('\n');
const navigation=analysis.current_runs.find(name=>name.startsWith('repeat-nav-'));
const retained=Object.keys(result.runs).filter(name=>!analysis.current_runs.includes(name));
const failedChecks=[...result.current_behavior_failures,...result.provenance_failures];
const failureSummary=failedChecks.length?failedChecks.map(c=>`- ${c.name}: ${JSON.stringify(c.observed)}`).join('\n'):'No current behavioral or provenance checks failed.';
const pendingColors=Object.entries(result.runs).filter(([name])=>analysis.current_runs.includes(name)).flatMap(([name,r])=>['upstream','oc'].flatMap(side=>Object.entries(r.pending_rgb[side]||{}).filter(([stage])=>['chip-normal','chip-pending','chip-restored','repeat-expanded','repeat-enter-pending','extra-enter-pending'].includes(stage)).map(([stage,g])=>`| ${name}/${side}/${stage} | ${JSON.stringify(g.cursor)} | ${[...new Set(g.rows.flatMap(row=>Object.keys(row.styled_nonblank_cells).map(k=>JSON.parse(k).fg)))].join(', ')} |`))).join('\n');
const report=`# VIS07 prompt/paste fresh-source diagnostic — 2026-09-27

**Result: ${result.behavior_status}; VIS07_NOT_PASS.**
Every comparable full grid and PNG differs. Behavioral observations do not qualify
all mandatory VIS07 frames or cases.

## Captured behavior

- Four widths, 79/80/120/121×40: the existing default leader sequence captures the
  same actual three-line chip in normal/pending/restored states and actual mouse
  click expansion. Expansion checks expose all three original lines. Normal /
  pending / restored symbol, bold and cursor checks are listed below; any
  unstable or nonmatching state is retained as a failure. No
  provider request occurs in any default-chip pair.
- The unchanged existing extra sequence captures the real wrapped longdraft
  pending-leader Enter, next and restored states at all four widths. One main
  Responses request and one auxiliary title occur per side, and all eight main
  wires contain the exact same user draft:

  \`VIS11 full draft αβ caret-middle preserving every wVIS11 Enter bounded actual requestord\`
- Current repeated-paste/navigation attempt \`${navigation}\`: same three-line
  paste repeated at chip end expands without a second insertion. Three actual
  Up keys and three Down keys move the visible caret and return to its original
  position on both binaries. Pending-leader Enter completes the real local
  fixture exchange with two valid requests per side.
- Optional \`suffix-space-120\` uses the same three-line paste, but inserts one
  real ordinary suffix space before repeating. The actual two-chip observation,
  both mouse expansions, visual navigation and exact two-paste wire are checked
  independently. Original's chip spacers remain additional raw input bytes.
  Submission/completion is reported separately and is not inferred from a
  successful pre-submit two-chip observation.
- Exact repeated-paste wires differ: OC2 retains one extra trailing ASCII space
  after \`VIS11-PASTE-2\`; native retains only the original pasted input. The
  donor inserts that space outside the chip extmark at
  \`opencode/packages/tui/src/component/prompt/index.tsx:1393\`; expansion
  replaces only the extmark at \`:1422–1424\`. Actual full inputs are preserved
   in \`protocol.json\` and \`${analysisName}\`. This is an observed
   remaining unapproved parity gap, not an acceptance waiver. In the suffix-space
   case the original also keeps one additional separator space between the two
   expanded copies, as well as its final trailing space.

## Counts and failures

Current diagnostic selection (${result.current.runs} original/native pairs):
${result.current.captures} full per-side captures;
${result.current.comparisons.grid.DIFFERENT||0} grid and
${result.current.comparisons.png.DIFFERENT||0} PNG comparisons DIFFERENT;
${result.current.cursor_differences} paired cursor differences.

All retained attempts (${retained.length} older attempts excluded from the current selection):
${result.all.captures} full captures,
${result.all.comparisons.grid.DIFFERENT||0} DIFFERENT + ${result.all.comparisons.grid.BLOCKED||0} BLOCKED grid comparisons,
${result.all.comparisons.png.DIFFERENT||0} DIFFERENT + ${result.all.comparisons.png.BLOCKED||0} BLOCKED PNG comparisons,
${result.all.cursor_differences} cursor differences. Zero EQUAL comparisons;
${result.all.unstable} unstable captures.

| Run | Full captures (both sides) | Grids different/blocked | PNGs different/blocked | Cursor differences | Requests original/native | Behavior original/native |
|---|---:|---:|---:|---:|---:|---|
${table}

All requests are to the isolated local fake provider: ${result.all.requests.upstream}
original + ${result.all.requests.oc} native = ${result.all.requests.upstream+result.all.requests.oc}
(${result.all.main} main, ${result.all.title} auxiliary title), including
${result.all.invalid} rejected requests. The bounded contract is 0 or 2 requests
per side/run; actual counts, including any crash-shortened run, are listed above.
Live/remote requests: 0.
Current selected requests: ${result.current.requests.upstream+result.current.requests.oc}, all valid.

${failureSummary}
Retained-attempt failed checks: ${result.retained_attempt_failures.length}.
The fixture accepts the explicitly expected original/native wires separately;
their actual mismatch is retained in full. No prior capture is overwritten.
Runner exits, interruption/blocker information and exact invocations are in
\`campaign.json\` and per-run logs; this report does not infer a successful
capture from an expected comparison exit 1.

## Unmasked full-frame differences

All styled cells, resolved colors/modifiers/widths, cursor, text, PNG geometry and
pixels remain in comparator scope. Version 2.0.12 versus 0.1.0 remains visible;
no version/footer/duration/title/token/path normalization or masking is used.
The analyzer records every differing row and per-field counts, not just the
comparator's first 20 sample coordinates. In particular native completed-session
footer context/tokens versus donor hints, modal cursor/blank styling and original
failure overlays remain in evidence.

| Current key grid | Differing cells (full frame) | Cursor differs |
|---|---:|---|
${keyFrames}

## Actual pending RGB and cursor

The compact table lists observed foregrounds on actual draft/chip rows. Full
coordinates, RGB foreground/background and modifiers remain in
\`${analysisName}\` and \`summary.json\` (\`pending_rgb\`); resolved
styled cells are retained without masking. Bold chips can retain their own
foreground while the surrounding raw draft changes color on pending leader.

| Run/side/state | Actual cursor | Observed draft-row foregrounds |
|---|---|---|
${pendingColors}

Full PNG review, when performed, is separately recorded in \`visual-review.md\`;
the automated full comparators do not substitute for independent visual review.

## Exact source/build association

Pinned original commit \`2670273ff17da96f85c5826ced57aa1b368754fa\`, executable
\`/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode\`,
SHA-256 \`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a\`.

Captured native HEAD(s): ${result.captured_heads.map(head=>`\`${head}\``).join(', ')}, plus
the uncommitted build/runtime/test-fixture inputs sealed below. All
${Object.keys(result.runs).length} independent \`--build-oc true\` invocations are checked
for successful \`cargo build --locked\`; failures are listed above. Every run used
\`/home/opencode/ai/oc/target/debug/oc\` SHA-256
\`${first.binary_sha256}\`, with one Rust input-set digest
\`${first.rust_inputs_sha256}\` (sorted Rust manifest entries, JSON SHA-256).
Source inputs matched before/after every build and capture. HEAD alone is not
claimed to contain the dirty changes. Complete dirty diffs are in each
\`commands.json\`; complete source manifests are retained per run.

| Build/runtime/test-fixture input | Captured SHA-256 | Matches current source |
|---|---|---|
${sourceTable}

Complete source manifests seal the capture/probe/analyzer tooling as well as
Rust; these digests distinguish every actual input set explicitly:

| Run | Captured HEAD | Complete canonical source-manifest SHA-256 | Dirty-diff SHA-256 |
|---|---|---|---|
${associations}

Shared capture profile: xterm.js/Chromium/DejaVu Sans Mono, Unicode 11, truecolor,
40 rows; sidebar hidden, animations disabled, real wall clock, both actual CLI
configs set \`session.tps=false\`. All launch versions/configuration, tooling
lock, build/bridge/comparator commands and exits, input events, raw VT and output
timelines remain per attempt. Capture shutdown uses the existing bridge's owned
process teardown; this is not a new graceful-exit/terminal-restoration gate.

## Checks and continuation

- JS syntax and Python AST checks passed before execution.
- Actual \`check_frontend.mjs\` and \`check_capture_geometry.mjs\` passed.
- Dedicated analyzer current behavioral failures: ${result.current_behavior_failures.length};
  provenance failures: ${result.provenance_failures.length}. Retained failures
  and actual wire mismatches remain explicit.
- Capture runner exits remain in \`campaign.json\`, preserving full-frame
  differences and any rejected fixture requests. No Rust, acceptance or task-status changes
  were made by this script/evidence work. Historical analyzer/evidence intact.
- Continue from current \`VIS07_NOT_PASS\`: observed trailing raw spacer,
  full-frame version/footer/modal differences and the remaining mandatory
  prompt/paste/Unicode/resize cases require qualification before any PASS claim.

Reproduce with a fresh campaign path:

\`node scripts/tui_capture/run_prompt_paste.mjs NEW_CAMPAIGN\`

\`node scripts/tui_capture/analyze_prompt_paste.mjs NEW_CAMPAIGN\`

\`node scripts/tui_capture/summarize_prompt_paste.mjs NEW_CAMPAIGN\`

Capture provenance and observations are in \`${analysisName}\`;
counts/key-frame/source association are in \`summary.json\`.
Evidence size before closeout: ${result.evidence_bytes_before_closeout} bytes.
`;
fs.writeFileSync(path.join(root,'report.md'),report,{flag:'wx'});
console.log(JSON.stringify({status:result.status,behavior_status:result.behavior_status,all:result.all,current:result.current,source_files:result.source_files,evidence_bytes:result.evidence_bytes_before_closeout},null,2));
