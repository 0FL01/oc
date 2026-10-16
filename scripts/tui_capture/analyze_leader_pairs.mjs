#!/usr/bin/env node
// Verify capture provenance and report whole-frame differences without masks.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
const root=path.resolve(process.argv[2]);
const fresh=process.argv.includes('--fresh');
const selected=name=>fresh?name+'-fresh'+(process.argv.includes('--matched-tps')&&['paired-extra','paired-enter-isolated'].includes(name)?'-tps-matched':name==='paired-extra'&&process.argv.includes('--extra-refreshed')?'-02':''):name;
const read=p=>JSON.parse(fs.readFileSync(p));
const sha=p=>createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const digest=x=>createHash('sha256').update(JSON.stringify(x)).digest('hex');
const result={status:'VIS11_NOT_PASS',runs:{},comparisons:{grid:{},png:{}},native_mismatches:[]};
for(const name of ['paired-default','paired-nested','paired-configured','paired-precedence','paired-legacy-v1-02','paired-extra','paired-enter-isolated']) {
  const dir=path.join(root,selected(name)),lock=read(path.join(dir,'capture.lock.json'));
  const build=read(path.join(dir,'commands.json')).filter(c=>c.argv.join(' ')==='cargo build --locked');
  assert.equal(build.length,1);assert.equal(build[0].exit_code,0);
  const manifest=read(path.join(dir,'source-manifest.json'));
  const rust=Object.fromEntries(Object.entries(manifest).filter(([p])=>p.startsWith('crates/')||p.startsWith('Cargo')||p==='rust-toolchain.toml'));
  const run=result.runs[name]={binary_sha256:lock.oc.executable_sha256,rust_inputs_sha256:digest(rust),source_manifest_sha256:lock.oc.source_manifest_sha256,build_exit:0,frames:[],sides:{}};
  for(const c of lock.captures) {
    const file=path.join(dir,c.path);assert.equal(sha(file+'.cells.json'),c.cells_sha256);assert.equal(sha(file+'.png'),c.png_sha256);assert.equal(sha(file+'.render.json'),c.render_sha256);
    const g=read(file+'.cells.json');assert.equal(g.columns,120);assert.equal(g.rows,40);
  }
  for(const a of lock.attempts.filter(x=>x.mode==='grid'||x.mode==='png')) {
    result.comparisons[a.mode][a.status]=(result.comparisons[a.mode][a.status]||0)+1;
    const diff=read(path.join(dir,a.scenario+'.'+a.mode+'-diff.json'));
    run.frames.push({scenario:a.scenario,mode:a.mode,status:a.status,different_cells:diff.different_cells,different_pixels:diff.different_pixels,cursor_differs:diff.cursor_differs,bbox:diff.difference_bbox_inclusive});
  }
  for(const side of ['upstream','oc']) {
    const checks=read(path.join(dir,side,'leader-checks.json')),protocol=read(path.join(dir,side,'protocol.json'));
    const observations=checks.observations;
    run.sides[side]={provider_requests:protocol.filter(e=>e.kind==='provider').map(e=>({operation:e.operation,valid:e.valid,actual_input:e.actual_input})),chip_expansion_verified:checks.chip_fulltext_preserved,observations};
    if(checks.idle) {
      assert.equal(checks.provider_requests,0);assert.equal(checks.idle.actual_inputs.length,1);
      const input=checks.idle.actual_inputs[0],writes=checks.idle.actual_output_writes.filter(e=>e.bytes>100);
      assert(writes.length>=2);
      const grid=stage=>read(path.join(dir,side,'leader-'+stage+'.cells.json'));
      const pending=grid('idle-pending'),restored=grid('idle-restored');
      assert.deepEqual(pending.cursor,restored.cursor);assert.deepEqual(pending.cells.map(r=>r.map(c=>c.symbol)),restored.cells.map(r=>r.map(c=>c.symbol)));
      run.sides[side].idle={only_input:input.base64,last_pending_first_restored_ms:checks.idle.samples.slice(-2).map(x=>x.elapsed_ms),raw_pty_writes_ms:writes.map(e=>(e.at_ns-input.at_ns)/1e6),preserved_full_grid_symbols_and_caret:true};
    }
  }
}
assert.equal(new Set(Object.values(result.runs).map(r=>r.rust_inputs_sha256)).size,1);
assert.equal(new Set(Object.values(result.runs).map(r=>r.binary_sha256)).size,1);
const extra=result.runs['paired-extra'].sides;
for(const stage of ['extra-ctrlc-next','extra-modal-pending']) {
  const ref=extra.upstream.observations.find(o=>o.name===stage),actual=extra.oc.observations.find(o=>o.name===stage);
  if(!fresh||ref.draft_row!==actual.draft_row||ref.draft_cell?.fg!==actual.draft_cell?.fg||JSON.stringify(ref.cursor)!==JSON.stringify(actual.cursor))result.native_mismatches.push({stage,reference:ref,actual,path:selected('paired-extra')+'/oc/leader-'+stage});
}
const enter=result.runs['paired-enter-isolated'].sides;
assert.equal(enter.upstream.provider_requests.length,2);assert.equal(enter.oc.provider_requests.length,fresh?2:0);
assert(enter.upstream.provider_requests.every(e=>e.valid));
if(fresh) {
  assert(enter.oc.provider_requests.every(e=>e.valid));
  const input=side=>side.provider_requests.find(e=>e.operation==='transcript').actual_input.filter(m=>m.role==='user').flatMap(m=>m.content.map(c=>c.text));
  assert.deepEqual(input(enter.upstream),input(enter.oc));
  result.isolated_enter_exact_user_input=input(enter.oc);
  result.fixed_cases={enter_exact_submission:true,ctrlc_and_modal:result.native_mismatches.length===0};
}else result.native_mismatches.push({stage:'isolated-enter',reference_provider_requests:2,native_provider_requests:0,path:'paired-enter-isolated/oc/leader-enter-only-next'});
for(const name of ['paired-default','paired-nested','paired-configured','paired-precedence','paired-legacy-v1-02']) {
  const run=result.runs[name];assert.equal(run.sides.upstream.chip_expansion_verified,true);assert.equal(run.sides.oc.chip_expansion_verified,false);
  result.native_mismatches.push({stage:'chip-expansion',scope:fresh?'VIS07_RETAINED_GAP':'native mismatch',path:selected(name)+'/oc/leader-chip-expanded-fulltext'});
}
fs.writeFileSync(path.join(root,process.argv.includes('--matched-tps')?'paired-analysis-tps-matched.json':process.argv.includes('--extra-refreshed')?'paired-analysis-final.json':'paired-analysis.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({status:result.status,comparisons:result.comparisons,binary_sha256:Object.values(result.runs)[0].binary_sha256,rust_inputs_sha256:Object.values(result.runs)[0].rust_inputs_sha256,mismatches:result.native_mismatches.map(x=>x.path)},null,2));
