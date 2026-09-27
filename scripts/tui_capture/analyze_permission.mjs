#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
const base=path.resolve(process.argv[2]), output=path.resolve(process.argv[3]);
const first=Number(process.argv[4]||1),last=Number(process.argv[5]||999);
if(fs.existsSync(output))throw Error('Immutable report already exists');
const read=p=>JSON.parse(fs.readFileSync(p));
const report={qualification:'DIAGNOSTIC_ONLY_FULL_VIS36_OPEN',scope:{first,last},runs:[],totals:{provider_requests:0,provider_completed:0,tool_calls:0,invalid_requests:0},gaps:['Full grids and PNGs differ; no masked parity claims','Complete width-by-action matrix remains open','Typed owner-preview digest and CoreEvent evidence are separate from this PTY/SQLite analysis','Atomic grant transaction failure not injected in these captures','Independent syscall/headless evidence and parent release gates must be linked separately; this analyzer does not qualify them'],live_requests:0};
for(const name of fs.readdirSync(base).filter(n=>/^permission20260927-\d+$/.test(n)&&Number(n.split('-').at(-1))>=first&&Number(n.split('-').at(-1))<=last).sort()) {
 const dir=path.join(base,name),lock=read(path.join(dir,'capture.lock.json'));
 const manifest=read(path.join(dir,'source-manifest.json'));
 const run={name,columns:lock.profile?.columns,mode:read(path.join(dir,'oc','bridge-spec.json')).permission_mode||'prompt',exit_code:lock.exit_code,build:read(path.join(dir,'commands.json')).filter(c=>c.argv[0]==='cargo'),source_manifest_sha256:lock.oc.source_manifest_sha256,binary_sha256:lock.oc.executable_sha256,sides:{},comparisons:lock.attempts,cursors:[],defects:[]};
 run.rust_inputs_sha256=createHash('sha256').update(JSON.stringify(Object.fromEntries(Object.entries(manifest).filter(([p])=>p.startsWith('crates/')||p.startsWith('Cargo')||p==='rust-toolchain.toml')))).digest('hex');
 for(const origin of ['upstream','oc']) {
  const protocol=read(path.join(dir,origin,'protocol.json')),checks=read(path.join(dir,origin,'permission-checks.json'));
  const requests=protocol.filter(e=>e.kind==='provider');
  const counts={provider_requests:requests.length,provider_completed:protocol.filter(e=>e.kind==='provider_completed').length,tool_calls:protocol.filter(e=>e.kind==='fixture_tool_call').length,invalid_requests:requests.filter(e=>!e.valid).length};
  for(const [key,count] of Object.entries(counts))report.totals[key]+=count;
  const snapshotsWithCounts=[],observed={provider_requests:0,provider_completed:0,tool_calls:0};
  for(const event of protocol){if(event.kind==='provider')observed.provider_requests++;if(event.kind==='provider_completed')observed.provider_completed++;if(event.kind==='fixture_tool_call')observed.tool_calls++;if(event.kind==='permission_snapshot')snapshotsWithCounts.push({stage:event.request_id,...observed});}
  run.sides[origin]={status:checks.status,reason:checks.reason,counts,observed_counts_at_snapshots:snapshotsWithCounts,operation_counts:requests.reduce((a,e)=>(a[e.case||e.operation]=(a[e.case||e.operation]||0)+1,a),{}),proofs:checks.checks.filter(c=>c.stage.endsWith('proof')),schemas:requests[0]?.request.tools,snapshots:checks.checks.filter(c=>c.snapshot).map(c=>({stage:c.stage,files:c.snapshot.files,cli:c.snapshot.cli,mcp_call_count:c.snapshot.mcp?.filter(r=>r.method==='tools/call').length,tables:c.snapshot.observations.flatMap(o=>o.tables.filter(t=>/permission/.test(t))),permission_data:c.snapshot.observations.map(o=>Object.fromEntries(Object.entries(o.data).filter(([t])=>/permission/.test(t)))),tool_operations:c.snapshot.observations.flatMap(o=>o.data.tool_operations||[]),events:c.snapshot.observations.flatMap(o=>(o.data.events||[]).filter(e=>JSON.stringify(e).match(/approval|permission|ToolCallStarted/)))}))};
 }
 for(const capture of lock.captures.filter(c=>c.origin==='oc')) {
  const counterpart=path.join(dir,'upstream',capture.scenario+'.cells.json');if(!fs.existsSync(counterpart))continue;
  const a=read(counterpart),b=read(path.join(dir,capture.path+'.cells.json'));
  run.cursors.push({scenario:capture.scenario,equal:JSON.stringify(a.cursor)===JSON.stringify(b.cursor),upstream:a.cursor,oc:b.cursor});
  if(/ask|fullscreen|selected|reject-completed/.test(capture.scenario)) {
   const samples=[];
   for(let y=0;y<a.rows;y++)for(let x=0;x<a.columns;x++)if(JSON.stringify(a.cells[y][x])!==JSON.stringify(b.cells[y][x])&&samples.length<12)samples.push({x,y,upstream:a.cells[y][x],oc:b.cells[y][x]});
   run.defects.push({scenario:capture.scenario,samples,rows:{upstream:a.cells.map((r,y)=>({y,text:r.map(c=>c.symbol).join('')})).filter(r=>r.text.trim()),oc:b.cells.map((r,y)=>({y,text:r.map(c=>c.symbol).join('')})).filter(r=>r.text.trim())}});
  }
 }
 report.runs.push(run);
}
report.comparator_counts=Object.fromEntries(['grid','png'].map(mode=>[mode,report.runs.flatMap(r=>r.comparisons).filter(c=>c.mode===mode).reduce((a,c)=>(a[c.status]=(a[c.status]||0)+1,a),{})]));
report.cursors={equal:report.runs.flatMap(r=>r.cursors).filter(c=>c.equal).length,total:report.runs.flatMap(r=>r.cursors).length};
fs.writeFileSync(output,JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify({output,totals:report.totals,comparators:report.comparator_counts,cursors:report.cursors,runs:report.runs.map(r=>({name:r.name,columns:r.columns,sides:Object.fromEntries(Object.entries(r.sides).map(([k,v])=>[k,{status:v.status,reason:v.reason,counts:v.counts}]))}))},null,2));
