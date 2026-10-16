#!/usr/bin/env node
// Read-only evidence analysis. Write a NEW report; never rewrite capture attempts.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
const base=path.resolve(process.argv[2]||'evidence/tui/recovery-v00');
const output=path.resolve(process.argv[3]||path.join(base,'apply-patch-analysis20260927-01.json'));
const read=p=>JSON.parse(fs.readFileSync(p));
const hash=v=>createHash('sha256').update(JSON.stringify(v)).digest('hex');
const report={qualification:'VIS35_DIAGNOSTIC_ONLY',runs:[],totals:{provider_requests:0,provider_completed:0,tool_calls:0,invalid_requests:0},gaps:['Whole-frame parity failed; no masks/crops.','Historical attempts01–10 lack native argument-stream running frames; executor-held state remains unqualified.','Stale multi-file case is preflight rejection, not confirmed-prefix partial failure.','PTY postcommit partial/cancelled/unknown not qualified; targeted executor tests cover partial effects.','VIS36 permission accept/reject unqualified.','A09 real-model patch authorship and release entry not run.','Snapshots unchanged is a filesystem/no-reexecution proof, not an OS read syscall audit.']};
for(let i=1;i<=Number(process.argv[4]||8);i++){
  const dir=path.join(base,'apply-patch20260927-'+String(i).padStart(2,'0'));
  const lock=read(path.join(dir,'capture.lock.json'));
  const commands=read(path.join(dir,'commands.json'));
  const manifest=read(path.join(dir,'source-manifest.json'));
  const entry={attempt:path.basename(dir),exit_code:lock.exit_code,columns:lock.profile?.columns,build:commands.filter(x=>x.argv[0]==='cargo'),source_manifest_sha256:lock.oc.source_manifest_sha256,rust_inputs_sha256:hash(Object.fromEntries(Object.entries(manifest).filter(([p])=>p.startsWith('crates/')||p.startsWith('Cargo')||p==='rust-toolchain.toml'))),binary_sha256:lock.oc.executable_sha256,comparisons:lock.attempts.reduce((a,x)=>(a[x.status]=(a[x.status]||0)+1,a),{}),sides:{}};
  entry.diff_settings=lock.profile?.settings.diffs;
  entry.comparisons_by_mode=Object.fromEntries(['grid','png'].map(mode=>[mode,lock.attempts.filter(x=>x.mode===mode).reduce((a,x)=>(a[x.status]=(a[x.status]||0)+1,a),{})]));
  entry.equal_comparisons=lock.attempts.filter(x=>x.status==='EQUAL');
  for(const origin of ['upstream','oc']){
    const protocolPath=path.join(dir,origin,'protocol.json');
    if(!fs.existsSync(protocolPath)){entry.sides[origin]={status:'NOT_RUN'};continue;}
    const protocol=read(protocolPath),checks=read(path.join(dir,origin,'apply-patch-checks.json'));
    const requests=protocol.filter(x=>x.kind==='provider');
    const calls=protocol.filter(x=>x.kind==='fixture_tool_call');
    const counts={provider_requests:requests.length,provider_completed:protocol.filter(x=>x.kind==='provider_completed').length,tool_calls:calls.length,invalid_requests:requests.filter(x=>!x.valid).length};
    for(const [k,v] of Object.entries(counts))report.totals[k]+=v;
     const side={status:checks.status,reason:checks.reason,counts,operation_counts:requests.reduce((a,x)=>(a[x.operation]=(a[x.operation]||0)+1,a),{}),actual_model_result_count:requests.flatMap(x=>x.actual_results||[]).length,proofs:checks.checks.filter(x=>x.stage.endsWith('-proof')),running:checks.checks.filter(x=>x.stage.endsWith('-running')).map(({stage,status,capture_status,reason})=>({stage,status,capture_status,reason})),replay:checks.checks.filter(x=>x.stage.endsWith('invariants'))};
     side.proofs=side.proofs.filter(p=>!p.stage.endsWith('-pending-proof'));
     side.pending_proofs=checks.checks.filter(p=>p.stage.endsWith('-pending-proof'));
     side.reconciliation=checks.checks.filter(p=>p.stage.endsWith('-reconcile'));
      side.provider_argument_deltas=protocol.filter(p=>p.kind==='fixture_argument_delta');
      side.hover_targets=checks.checks.filter(p=>p.stage.endsWith('-hover-target'));
    const snapshots=checks.checks.filter(x=>x.snapshot);
    const final=snapshots.at(-1)?.snapshot;
    side.final_files=final?.files;
    side.declared_patch_schema=requests.find(x=>x.operation==='patch')?.request.tools.find(x=>x.name===(origin==='oc'?'apply_patch':'patch'));
    if(i>=3&&lock.exit_code!==2){
      assert.equal(checks.status,'PASS');assert.deepEqual(counts,{provider_requests:21,provider_completed:21,tool_calls:10,invalid_requests:0});
      assert.equal(side.actual_model_result_count,10);
      assert.equal(side.declared_patch_schema.type,'function');assert.equal(side.declared_patch_schema.parameters.properties.patchText.type,'string');
      assert.deepEqual(Object.keys(side.declared_patch_schema.parameters.properties),['patchText']);
      assert.equal(side.proofs.length,10);assert(side.proofs.every(p=>p.bytes_presence_pass));
      assert(side.proofs.every(p=>p.update_mode_preserved));
      assert.equal(final.files['prefix.txt'].present,false);assert.equal(final.files['move.txt'].present,false);assert.equal(final.files['delete.txt'].present,false);
      assert.equal(final.files['moved.txt'].mode,origin==='oc'?'0o751':'0o664');
       assert.equal(side.replay.length,2);assert(side.replay.every(p=>p.files_unchanged&&p.tool_calls_unchanged&&p.provider_requests_unchanged));
       if(i>=11){
         assert.equal(side.provider_argument_deltas.length,20);
         assert(calls.every(c=>JSON.parse(side.provider_argument_deltas.filter(d=>d.call_id===c.call_id).map(d=>d.delta).join('')).patchText===c.arguments.patchText));
         side.pending_qualified=side.pending_proofs.length===10&&side.pending_proofs.every(p=>p.files_unchanged&&(origin!=='oc'||p.pending_label_count===1&&p.durable_operations_unchanged&&p.effect_rows_unchanged));
         if(origin==='oc'){
           assert(side.pending_qualified);
           assert.equal(side.reconciliation.length,10);
           assert(side.reconciliation.every(p=>p.pending_label_count===0&&p.durable_operation_count===1));
           assert(side.replay.at(-1).durable_operations_unchanged&&side.replay.at(-1).effect_rows_unchanged&&side.replay.at(-1).pending_label_count===0);
           const links=final.observations.flatMap(o=>o.data.turns||[]).flatMap(t=>JSON.parse(t.result||'{}').display_parts||[]).filter(p=>p.tool).map(p=>p.tool);
           assert.equal(links.length,10);assert.equal(new Set(links).size,10);
           side.durable_display_links={count:links.length,unique:new Set(links).size};
         }
       }
      if(origin==='upstream'){
        const executed=final.executor_hook.filter(e=>e.kind==='execute.after');assert.equal(executed.length,10);
        side.original_executor_counts=executed.reduce((a,x)=>(a[x.status]=(a[x.status]||0)+1,a),{});
        const admissions=final.executor_hook.filter(e=>e.kind==='admission');assert(admissions.every(e=>!e.tools.includes('write')&&!e.tools.includes('edit')));
      }
      if(i>=8&&origin==='oc'){
        const effects=final.observations.flatMap(o=>o.data.patch_effects||[]);
        const before=checks.checks.find(x=>x.stage==='before-reopen').snapshot.observations.flatMap(o=>o.data.patch_effects||[]);
        assert.deepEqual(effects,before);
        assert.equal(effects.length,9);
        assert(effects.every(e=>!e.op_id.endsWith('call_vis35_denied')));
        const files=effects.flatMap(e=>JSON.parse(e.metadata).files);
        assert(files.every(f=>f.algorithm==='Minimal'));
        assert(effects.every(e=>Buffer.byteLength(e.metadata)<=65536));
        const update=files.find(f=>f.path==='update.txt');
        assert.deepEqual(update.hunks.map(h=>[h.old.start,h.old.count,h.new.start,h.new.count]),[[4,9,4,9],[16,9,16,9]]);
        assert.deepEqual(update.hunks.flatMap(h=>h.lines).filter(l=>l.kind==='Added').map(l=>l.new_line),[8,20]);
        assert.equal(files.filter(f=>f.path==='prefix.txt').length,0);
        side.persisted_metadata={rows:effects.length,identical_before_redo_and_restart:true,max_serialized_bytes:Math.max(...effects.map(e=>Buffer.byteLength(e.metadata))),algorithms:[...new Set(files.map(f=>f.algorithm))],multihunk_ranges:update.hunks.map(h=>({old:h.old,new:h.new})),denied_operation_has_no_effect_row:true};
      }
    }
    entry.sides[origin]=side;
  }
  entry.cursor_comparison={};
  entry.frame_diagnostics={};
  for(const capture of lock.captures.filter(c=>c.origin==='oc')){
    const p=path.join(dir,'upstream',capture.scenario+'.cells.json');if(!fs.existsSync(p))continue;
    const upstream=read(p),native=read(path.join(dir,capture.path+'.cells.json'));
     entry.cursor_comparison[capture.scenario]={equal:JSON.stringify(upstream.cursor)===JSON.stringify(native.cursor),upstream:upstream.cursor,oc:native.cursor};
     if(i>=11){
       const diff=path.join(dir,capture.scenario+'.grid-diff.json');
        entry.frame_diagnostics[capture.scenario]={grid:fs.existsSync(diff)?read(diff):null,upstream_rows:upstream.cells.map(row=>row.map(c=>c.symbol).join('')).filter(t=>/Created|Deleted|Updated|Moved|arguments streaming|Patching|Applying|old move|new move/.test(t)),oc_rows:native.cells.map(row=>row.map(c=>c.symbol).join('')).filter(t=>/Created|Deleted|Updated|Moved|arguments streaming|Patching|Applying|old move|new move/.test(t))};
        if(i>=19){
          const diagnostic=entry.frame_diagnostics[capture.scenario];
          diagnostic.difference_rows=(diagnostic.grid?.samples||[]).map(s=>({x:s.x,y:s.y,fields:s.different_fields,upstream:upstream.cells[s.y].map(c=>c.symbol).join(''),oc:native.cells[s.y].map(c=>c.symbol).join('')}));
        }
     }
  }
  if(i===1)entry.semantic_failure='External 120s watchdog interrupted before native launch; regex-only running shots can actually be completed. Do not use run01 to qualify running.';
  if(i===2)entry.semantic_failure='External 360s watchdog interrupted native restart capture; original lifecycle passed. No restart qualification from this attempt.';
  if(i<=8)entry.hover_limitation='Historical multifile-hover targeted visible replace.txt from the prior operation. It does not qualify hover of multi-a.txt; current runner records unique current-file coordinates.';
  if(i===14)entry.semantic_failure='External 600s watchdog around12–14 interrupted original replay, native NOT_RUN; six original pending snapshots were already after execution. Preserve failures;15 uses fresh root and longer real SSE hold.';
  report.runs.push(entry);
}
report.same_native_binary_across_runs02_onward=new Set(report.runs.slice(1).map(r=>r.binary_sha256)).size===1;
report.source_groups=Object.groupBy(report.runs,r=>r.rust_inputs_sha256);
report.same_rust_inputs_across_all_runs=new Set(report.runs.map(r=>r.rust_inputs_sha256)).size===1;
report.transport_count_note='Observed 21 requests/21 completions/10 calls per completed side, including 1 title and 20 patch requests. No transport adapter was added and no general request-count equivalence claim is made.';
report.fresh_summary=report.runs.slice(10).map(r=>({attempt:r.attempt,exit:r.exit_code,columns:r.columns,binary:r.binary_sha256,rust:r.rust_inputs_sha256,comparisons:r.comparisons,cursor_pairs:Object.keys(r.cursor_comparison).length,cursors_equal:Object.values(r.cursor_comparison).filter(c=>c.equal).length,sides:Object.fromEntries(Object.entries(r.sides).map(([k,v])=>[k,{status:v.status,counts:v.counts,pending_qualified:v.pending_qualified,pending_snapshots:v.pending_proofs?.length,metadata:v.persisted_metadata,durable_display_links:v.durable_display_links}])),multihunk:r.frame_diagnostics['apply-patch-multihunk-completed'],move:r.frame_diagnostics['apply-patch-move-completed']}));
report.corrected_campaign=report.runs.slice(18).map(r=>({attempt:r.attempt,exit:r.exit_code,columns:r.columns,diff_settings:r.diff_settings,binary:r.binary_sha256,rust:r.rust_inputs_sha256,manifest:r.source_manifest_sha256,build_exits:r.build.map(b=>b.exit_code),comparisons:r.comparisons_by_mode,equal:r.equal_comparisons,cursor_pairs:Object.keys(r.cursor_comparison).length,cursors_equal:Object.values(r.cursor_comparison).filter(c=>c.equal).length,grid_differences:Object.fromEntries(['multihunk','delete','move','replace','multifile'].map(n=>[n,r.frame_diagnostics['apply-patch-'+n+'-completed']?.grid?.different_cells])),sides:Object.fromEntries(Object.entries(r.sides).map(([k,v])=>[k,{status:v.status,counts:v.counts,pending_qualified:v.pending_qualified,move_hover:v.hover_targets.find(h=>h.stage==='move-hover-target'),replay:v.replay,links:v.durable_display_links}]))}));
report.current_rust_source_matches_latest_manifest=Object.entries(read(path.join(base,report.runs.at(-1).attempt,'source-manifest.json'))).filter(([p])=>p.startsWith('crates/')||p.startsWith('Cargo')||p==='rust-toolchain.toml').every(([p,digest])=>createHash('sha256').update(fs.readFileSync(path.resolve(p))).digest('hex')===digest);
assert(report.current_rust_source_matches_latest_manifest,'Current Rust changed since latest source-built capture');
fs.writeFileSync(output,JSON.stringify(report,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({output,totals:report.totals,runs:report.runs.map(r=>({attempt:r.attempt,exit:r.exit_code,sides:Object.fromEntries(Object.entries(r.sides).map(([k,v])=>[k,v.status])),comparisons:r.comparisons})),same_rust_inputs:report.same_rust_inputs_across_all_runs,corrected_campaign:report.corrected_campaign},null,2));
