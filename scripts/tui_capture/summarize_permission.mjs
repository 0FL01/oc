#!/usr/bin/env node
// Independent post-capture assertions; never modify historical attempts.
import fs from 'node:fs';
import path from 'node:path';
const base=path.resolve(process.argv[2]),analysis=JSON.parse(fs.readFileSync(process.argv[3])),output=path.resolve(process.argv[4]);
if(fs.existsSync(output))throw Error('Immutable output exists');
const grants=s=>(s?.permission_data||[]).flatMap(o=>o.permission_grants||o.permission||[]);
const report={totals:analysis.totals,comparators:analysis.comparator_counts,cursors:analysis.cursors,runs:[],positions:[],gaps:analysis.gaps};
for(const run of analysis.runs) {
 const sides={};
 for(const [origin,side] of Object.entries(run.sides)) {
  const snapshot=stage=>side.snapshots.find(s=>s.stage===stage);
  const coreStages=['seed','once-pending','once-effects','reject-pending','reject-effects','always-pending','always-effects','restart-effects','mixed-effects'];
   const grantCounts=Object.fromEntries(coreStages.map(s=>[s,snapshot(s)?grants(snapshot(s)).length:null]));
  const native=origin==='oc';
  const pauseSnapshots=side.snapshots.filter(s=>s.stage.endsWith('pending'));
   const noStarted=pauseSnapshots.length?pauseSnapshots.every(s=>!s.tool_operations.some(o=>String(o.id).endsWith('call_vis36_'+s.stage.replace('-pending','').replace('childroot','childread')))):null;
  const child=side.proofs.find(p=>p.stage==='childroot-proof');
  sides[origin]={status:side.status,reason:side.reason,counts:side.counts,operation_counts:side.operation_counts,grant_counts:grantCounts,saved_grants:grants(snapshot('always-effects')),grants_survive_restart:snapshot('always-effects')&&snapshot('restart-effects')?JSON.stringify(grants(snapshot('always-effects')))===JSON.stringify(grants(snapshot('restart-effects'))):null,no_operation_row_for_current_ask:noStarted,child_feedback_actual_results:child?.actual_results,mcp_proof:side.proofs.find(p=>p.stage==='mcp-proof'),auto_once_proof:side.proofs.find(p=>p.stage==='autoonce-proof'),mode_proofs:side.proofs.filter(p=>p.stage.startsWith('auto-')),counts_at_snapshots:side.observed_counts_at_snapshots};
 }
  report.runs.push({name:run.name,columns:run.columns,mode:run.mode,exit_code:run.exit_code,binary_sha256:run.binary_sha256,source_manifest_sha256:run.source_manifest_sha256,rust_inputs_sha256:run.rust_inputs_sha256,build:run.build,sides});
}
for(const name of analysis.runs.map(r=>r.name))for(const scenario of ['once-ask','once-fullscreen','always-selected','child-feedback-draft','settings-autoaccept','settings-auto-first-escape','read-ask','shell-ask','mcp-ask','url-ask','reject-completed','failure']) {
 const entry={name,scenario,sides:{}};
 for(const origin of ['upstream','oc']) {
  const p=path.join(base,name,origin,'permission-'+scenario+'.cells.json');if(!fs.existsSync(p))continue;
  const frame=JSON.parse(fs.readFileSync(p));
  const targets={};
    for(const token of ['△','!','┃','Permission required','→ Edit','Allow once','Always allow','Reject','Tell OpenCode','VIS36-FEEDBACK','this correction.','enter confirm','ctrl+f fullscreen','ctrl+f minimize','Permissions','auto accept','Autoaccept','This will always','Session','{','Patching','approval.txt','apply_patch','Settings','Search','# Patch failed','The user declined','→ WebFetch','→ Webfetch','URL:']) {
   const symbols=[...token],matches=[];
   for(let y=0;y<frame.rows;y++)for(let x=0;x<=frame.columns-symbols.length;x++)if(symbols.every((s,i)=>frame.cells[y][x+i].symbol===s))matches.push({x,y,cell:frame.cells[y][x]});
   if(matches.length)targets[token]=matches;
  }
  entry.sides[origin]={columns:frame.columns,cursor:frame.cursor,targets};
 }
 report.positions.push(entry);
}
const headless=process.argv[5]?JSON.parse(fs.readFileSync(path.join(base,process.argv[5],'report.json'))):{binary_sha256:null,cases:[]};
report.headless={binary_sha256:headless.binary_sha256,cases:headless.cases.map(({case:c,exit_code,effect,grants,counts,pass})=>({case:c,exit_code,effect,grants,counts,pass}))};
report.invalid_cursor_observations=analysis.runs.flatMap(r=>r.cursors.filter(c=>c.upstream.x<0||c.upstream.x>=r.columns||c.oc.x<0||c.oc.x>=r.columns).map(c=>({name:r.name,columns:r.columns,...c})));
report.all_requests_including_headless=Object.fromEntries(Object.entries(analysis.totals).map(([key,v])=>[key,v+headless.cases.reduce((sum,c)=>sum+c.counts[key],0)]));
fs.writeFileSync(output,JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify({output,totals:report.all_requests_including_headless,comparators:report.comparators,cursors:report.cursors,runs:report.runs.map(r=>({name:r.name,mode:r.mode,sides:Object.fromEntries(Object.entries(r.sides).map(([o,s])=>[o,{status:s.status,counts:s.counts,grant_counts:s.grant_counts,grants_survive_restart:s.grants_survive_restart}]))}))},null,2));
console.log('SOURCE ASSOCIATIONS',JSON.stringify(report.runs.map(r=>({name:r.name,binary:r.binary_sha256,rust:r.rust_inputs_sha256,manifest:r.source_manifest_sha256,build_exits:r.build.map(c=>c.exit_code)}))));
for(const p of report.positions.filter(p=>analysis.runs.filter(r=>r.mode==='prompt').slice(0,2).some(r=>r.name===p.name))) {
 const brief=Object.fromEntries(Object.entries(p.sides).map(([origin,s])=>[origin,Object.fromEntries(Object.entries(s.targets).filter(([t])=>t!=='┃').map(([t,m])=>[t,m.map(({x,y})=>[x,y])]))]));
 console.log('ZERO-BASED',p.name,p.scenario,JSON.stringify(brief));
 if(p.scenario==='once-ask')console.log('STYLES',p.name,JSON.stringify(Object.fromEntries(Object.entries(p.sides).map(([o,s])=>[o,Object.fromEntries(Object.entries(s.targets).filter(([t])=>['!','→ Edit','Permission required','Allow once'].includes(t)).map(([t,m])=>[t,m]))]))));
}
console.log('INVALID CURSORS',JSON.stringify(report.invalid_cursor_observations));
