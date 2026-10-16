#!/usr/bin/env node
// Read-only rejected-card/effect/output audit; storage is not a typed event trace.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
const [base,output,firstArg,lastArg,historyArg]=process.argv.slice(2);
const first=Number(firstArg),last=Number(lastArg),historyFirst=Number(historyArg);
if(fs.existsSync(output))throw Error('Immutable output exists');
const read=p=>JSON.parse(fs.readFileSync(p));
const report={scope:'Latest native PTY/readonly SQLite rejection audit. Historical outputs are a regression baseline only. No typed Started/CoreEvent/CurrentIntent qualification.',runs:[]};
for(let n=first;n<=last;n++){
 const name=`permission20260927-${n}`,history=`permission20260927-${historyFirst+n-first}`;
 const checks=read(path.join(base,name,'oc','permission-checks.json'));
 const snapshot=stage=>checks.checks.find(c=>c.stage===stage).snapshot;
 const data=(s,t)=>s.observations.flatMap(o=>o.data[t]||[]);
 const before=snapshot('once-effects'),pending=snapshot('reject-pending'),held=snapshot('reject-held'),after=snapshot('reject-effects');
 const op=data(after,'tool_operations').filter(o=>o.id.endsWith('call_vis36_reject'));
 assert.equal(op.length,1);assert.equal(op[0].state,'denied');
 assert.equal(op[0].output,'{"status":"permission_rejected","feedback":null}');
 assert.deepEqual(before.files,pending.files);assert.deepEqual(pending.files,held.files);assert.deepEqual(held.files,after.files);
 const effects=data(after,'patch_effects').filter(e=>e.op_id===op[0].id);assert.equal(effects.length,0);
 for(const s of [pending,held])assert(!data(s,'tool_operations').some(o=>o.id.endsWith('call_vis36_reject')));
 const savedOutputs=data(after,'turns').flatMap(r=>JSON.parse(r.result).input||[]).filter(r=>r.type==='function_call_output'&&r.call_id==='call_vis36_reject');
 assert(savedOutputs.length>0);assert(savedOutputs.every(r=>r.output===op[0].output));
 const oldChecks=read(path.join(base,history,'oc','permission-checks.json'));
 const oldSnapshot=oldChecks.checks.find(c=>c.stage==='reject-effects').snapshot;
 const oldOp=data(oldSnapshot,'tool_operations').find(o=>o.id.endsWith('call_vis36_reject'));
 assert.equal(op[0].output,oldOp.output);
 const protocol=read(path.join(base,name,'oc','protocol.json'));
 const requests=protocol.filter(e=>e.kind==='provider'&&e.case==='reject');assert.equal(requests.length,1);
 report.runs.push({name,historical_output_baseline:history,terminal_operation:op[0],persisted_function_outputs:savedOutputs,files_unchanged:true,pending_operation_rows:0,held_operation_rows:0,rejected_patch_effect_rows:effects.length,reject_provider_requests:requests.length,output_matches_historical_source:true});
}
report.status='PASS';fs.writeFileSync(output,JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report,null,2));
