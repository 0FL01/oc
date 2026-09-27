#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
const base=path.resolve(process.argv[2]),output=path.resolve(process.argv[3]);
if(fs.existsSync(output))throw Error('Immutable output exists');
const read=p=>JSON.parse(fs.readFileSync(p));
const report={checks:[],qualification:'TARGETED_LIFECYCLE_ONLY_FULL_VIS36_OPEN'};
const promptRuns=(process.argv[4]||'permission20260927-08,permission20260927-09').split(',');
const autoRuns=(process.argv[5]||'permission20260927-13,permission20260927-14').split(',');
const headlessRun=process.argv[6]||'permission-headless20260927-01';
report.scope={promptRuns,autoRuns,headlessRun};
for(const name of promptRuns)for(const origin of ['upstream','oc']) {
 const dir=path.join(base,name,origin),checks=read(path.join(dir,'permission-checks.json')),protocol=read(path.join(dir,'protocol.json'));
 assert.equal(checks.status,'PASS');
 const snapshot=stage=>checks.checks.find(c=>c.stage===stage).snapshot;
 const grants=s=>s.observations.flatMap(o=>o.data.permission_grants||o.data.permission||[]);
 for(const stage of ['seed','once-pending','once-effects','reject-pending','reject-effects','always-pending'])assert.equal(grants(snapshot(stage)).length,0);
 assert.equal(grants(snapshot('always-effects')).length,1);
 assert.deepEqual(grants(snapshot('always-effects')),grants(snapshot('restart-effects')));
 assert.deepEqual(snapshot('once-effects').files,snapshot('reject-effects').files);
 assert.deepEqual(snapshot('restart-effects').files,snapshot('mixed-effects').files);
  for(const caseName of ['once','reject','always',...(Number(name.split('-').at(-1))>=23?['mcp','childroot','read','shell','glob','url']:[])]) {
   const a=snapshot(caseName+'-pending'),b=snapshot(caseName+'-held');assert.deepEqual(a.files,b.files);
   const ai=protocol.findIndex(e=>e.kind==='permission_snapshot'&&e.request_id===caseName+'-pending'),bi=protocol.findIndex(e=>e.kind==='permission_snapshot'&&e.request_id===caseName+'-held');
  assert(!protocol.slice(ai+1,bi).some(e=>e.kind==='provider'));
 }
 assert.equal(snapshot('mcp-pending').mcp.filter(e=>e.method==='tools/call').length,0);
 assert.equal(snapshot('mcp-effects').mcp.filter(e=>e.method==='tools/call').length,1);
 const child=protocol.find(e=>e.kind==='provider'&&e.case==='childread'&&e.actual_results?.length);
 const feedbackInContext=JSON.stringify(child.request).includes('VIS36-FEEDBACK');
 if(origin==='oc')assert(feedbackInContext);
  assert.deepEqual(snapshot('childroot-pending').files,snapshot('childroot-effects').files);
  for(const c of checks.checks.filter(c=>c.stage.endsWith('-pause-proof'))) {
   assert.equal(c.files_unchanged,true);
   if('no_extra_provider_requests' in c)assert.equal(c.no_extra_provider_requests,true);
   if('no_started' in c)assert.equal(c.no_started,true);
  }
  if(Number(name.split('-').at(-1))>=17){
   const visible=checks.checks.find(c=>c.stage==='child-feedback-visibility-proof');assert.equal(visible.visible,visible.expected);
   const home=checks.checks.find(c=>c.stage==='once-newhome-pause-proof');assert(home.files_unchanged&&home.no_extra_provider_requests&&home.attention.length===1);
   assert.deepEqual(snapshot('once-newhome-held').files,snapshot('once-pending').files);
   const query='call_vis36_';
   for(const c of checks.checks.filter(c=>c.stage.endsWith('-pending')))assert(!c.snapshot.observations.flatMap(o=>o.data.tool_operations||[]).some(o=>String(o.op_id||o.id).endsWith(query+c.stage.replace('-pending','').replace('childroot','childread'))));
   const exits=protocol.filter(e=>e.kind==='exit');assert.equal(exits.length,2);assert(exits.every(e=>e.code===0||e.returncode===0||e.exit_code===0));
  }
 const operationCounts=protocol.filter(e=>e.kind==='provider'&&e.operation!=='title').reduce((a,e)=>(a[e.case||e.operation]=(a[e.case||e.operation]||0)+1,a),{});
  assert.equal(operationCounts.reject,1);assert.equal(operationCounts.glob,1);assert.equal(operationCounts.promptagain,1);assert.equal(operationCounts.once,2);assert.equal(operationCounts.childread,2);
  if(Number(name.split('-').at(-1))>=23){for(const c of ['always','restart','mixed','mcp','childroot','read','shell','autoonce'])assert.equal(operationCounts[c],2);assert.equal(operationCounts.url,1);}
 report.checks.push({name,origin,grant_rows_after_always:grants(snapshot('always-effects')),feedback_present_in_actual_child_provider_context:feedbackInContext,child_actual_results:child.actual_results,operation_counts:operationCounts,pass:true});
}
for(const name of autoRuns)for(const origin of ['upstream','oc']) {
 const dir=path.join(base,name,origin),checks=read(path.join(dir,'permission-checks.json'));
 assert.equal(checks.status,'PASS');
 for(const c of checks.checks.filter(c=>c.snapshot))assert.equal(c.snapshot.observations.flatMap(o=>o.data.permission_grants||o.data.permission||[]).length,0);
 const mixed=checks.checks.find(c=>c.stage==='auto-mixed-proof');assert(mixed.files_unchanged);assert(/denied/i.test(JSON.stringify(mixed.actual_results)));
 report.checks.push({name,origin,auto_mode_saves_zero_grants:true,pass:true});
}
const headless=read(path.join(base,headlessRun,'report.json'));assert(headless.cases.every(c=>c.pass));
const binaries=[...promptRuns,...autoRuns].map(n=>read(path.join(base,n,'capture.lock.json')).oc.executable_sha256);
assert.equal(new Set(binaries).size,1);assert.equal(headless.binary_sha256,binaries[0]);report.binary_sha256=binaries[0];
const sourceHashes=[...promptRuns,...autoRuns].map(n=>{const manifest=read(path.join(base,n,'source-manifest.json'));return createHash('sha256').update(JSON.stringify(Object.fromEntries(Object.entries(manifest).filter(([p])=>p.startsWith('crates/')||p.startsWith('Cargo')||p==='rust-toolchain.toml')))).digest('hex');});
assert.equal(new Set(sourceHashes).size,1);report.rust_inputs_sha256=sourceHashes[0];
report.headless=headless.cases.map(c=>({case:c.case,pass:c.pass}));
fs.writeFileSync(output,JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify(report,null,2));
