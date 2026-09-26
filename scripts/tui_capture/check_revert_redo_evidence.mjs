#!/usr/bin/env node
// Read-only seal/behavior audit of one complete actual VIS33 capture.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
const root=path.resolve(process.argv[2]);
const read=name=>JSON.parse(fs.readFileSync(path.join(root,name),'utf8'));
const lock=read('capture.lock.json');
const sha=file=>createHash('sha256').update(fs.readFileSync(path.join(root,file))).digest('hex');
assert.deepEqual(lock.oc.build_command,['cargo','build','--locked']);
assert(read('commands.json').some(c=>c.argv.join(' ')==='cargo build --locked'&&c.exit_code===0));
const summary={root,behavior:{},captures:lock.captures.length,comparisons:lock.attempts.length};
let prompts,responses;
for(const origin of ['upstream','oc']) {
  const checks=read(origin+'/revert-redo-checks.json');assert.equal(checks.status,'PASS');
  const protocol=read(origin+'/protocol.json');
  const requests=protocol.filter(e=>e.kind==='provider');
  const completed=protocol.filter(e=>e.kind==='provider_completed');
  assert.equal(requests.length,4);assert.equal(completed.length,4);assert(requests.every(r=>r.valid));
  assert.equal(requests.filter(r=>r.operation==='title').length,1);
  const transcript=requests.filter(r=>r.operation==='transcript');
  assert.equal(transcript.length,3);assert(transcript.every(r=>r.tool_result_count===0));
  const actualPrompts=transcript.map(r=>r.fixture_prompt),actualResponses=transcript.map(r=>r.fixture_response);
  if(prompts){assert.deepEqual(actualPrompts,prompts);assert.deepEqual(actualResponses,responses);}
  prompts=actualPrompts;responses=actualResponses;
  assert(checks.checks.filter(c=>c.zero_extra_calls!==undefined).every(c=>c.zero_extra_calls));
  const snapshots=protocol.filter(e=>e.kind==='revert_snapshot');
  for(const s of snapshots) {
    assert.deepEqual(s.project_files,checks.immutable_files.project);
    assert.deepEqual(s.config_files,checks.immutable_files.config);
    const expected=/restored-db|completed-db|final-db/.test(s.request_id)?[3,0]:[1,2];
    assert(s.observations.flatMap(o=>o.rows).some(r=>r.archived_users===3&&r.visible_users===expected[0]&&r.reverted_users===expected[1]));
  }
  assert.deepEqual(protocol.filter(e=>e.kind==='exit').map(e=>[e.generation,e.code,e.termination]),[[0,0,'natural'],[1,0,'natural']]);
  summary.behavior[origin]={status:checks.status,requests:4,transcript:3,title:1,extra_action_requests:0,db_snapshots:snapshots.length,clean_exits:2};
}
for(const c of lock.captures) {
  assert.equal(c.status,'CAPTURED');
  assert.equal(sha(c.path+'.cells.json'),c.cells_sha256);
  assert.equal(sha(c.path+'.png'),c.png_sha256);
  assert.equal(sha(c.path+'.render.json'),c.render_sha256);
}
assert.equal(lock.captures.length,58);
assert.equal(lock.attempts.length,58);
assert(lock.attempts.every(a=>a.status==='DIFFERENT'));
console.log(JSON.stringify(summary,null,2));
