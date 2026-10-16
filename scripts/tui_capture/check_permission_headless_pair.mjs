#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
const [base,nativeName,referenceName,output]=process.argv.slice(2);
if(fs.existsSync(output))throw Error('Immutable output exists');
const read=p=>JSON.parse(fs.readFileSync(p));
const native=read(path.join(base,nativeName,'report.json')),reference=read(path.join(base,referenceName,'report.json'));
const rows=[];
for(const name of ['configauto-no-consumer','cli-auto-once','cli-auto-deny']){
 const a=native.cases.find(c=>c.case===name),b=reference.cases.find(c=>c.case===name);
 assert(a.pass);assert(b.targeted_effect_pass);assert(!b.timed_out);
 for(const c of [a,b]){
  assert.equal(c.effect,name==='cli-auto-once');assert.equal(c.exit_code,name==='configauto-no-consumer'?1:0);
  assert.equal(c.grants.length,0);assert.equal(c.counts.invalid_requests,0);assert.equal(c.counts.tool_calls,1);
  assert.equal(c.counts.provider_requests,name==='configauto-no-consumer'?2:3);assert.equal(c.counts.provider_completed,c.counts.provider_requests);
 }
 const snapshot=read(path.join(base,referenceName,name+'.snapshot.json'));
 const permissionTables=snapshot.observations.flatMap(o=>o.tables.filter(t=>t.includes('permission')));
 assert(permissionTables.length>0,'Original permission DB must actually be observed');
 for(const o of snapshot.observations)for(const table of o.tables.filter(t=>t.includes('permission'))){assert(Object.hasOwn(o.data,table));assert.equal(o.data[table].length,0);}
 if(name==='configauto-no-consumer'){
  assert(fs.readFileSync(path.join(base,referenceName,name+'.stderr'),'utf8').includes('auto-rejecting'));
  assert(fs.readFileSync(path.join(base,nativeName,name+'.stderr'),'utf8').includes('no consumer'));
 }
 if(name==='cli-auto-deny'){
  const results=read(path.join(base,nativeName,name+'.protocol.json')).filter(e=>e.kind==='provider').flatMap(e=>e.actual_results||[]);
  assert(/denied/i.test(JSON.stringify(results)));assert(/permission.rejected/.test(JSON.stringify(b.actual_results)));
 }
 rows.push({name,native:{argv:a.argv,counts:a.counts,exit_code:a.exit_code,effect:a.effect},reference:{argv:b.argv,counts:b.counts,exit_code:b.exit_code,effect:b.effect,permission_tables:permissionTables}});
}
const report={status:'PASS',qualification:'Three bounded paired headless behaviors; no renderer or typed event qualification',native_binary_sha256:native.binary_sha256,reference_binary_sha256:reference.binary_sha256,rows};
fs.writeFileSync(output,JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report,null,2));
