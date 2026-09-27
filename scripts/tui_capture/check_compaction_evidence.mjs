#!/usr/bin/env node
// Read-only verification of actual binary evidence, independent of UI predicates.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';

const base=path.resolve(process.argv[2]||'evidence/tui/recovery-v00');
const read=p=>JSON.parse(fs.readFileSync(p,'utf8'));
const sha=p=>createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const canonical=v=>JSON.stringify(v,(_,x)=>x&&typeof x==='object'&&!Array.isArray(x)?Object.fromEntries(Object.entries(x).sort(([a],[b])=>a.localeCompare(b))):x);
const latest=process.argv.includes('--latest');
const optional=process.argv.includes('--optional');
if(optional&&!latest)throw Error('--optional requires --latest');
const final=latest||process.argv.includes('--final');
const fresh=final||process.argv.includes('--fresh');
const runs=latest?{manual:'18',threshold:'19',overflow:'20'}:final?{manual:'15',threshold:'16',overflow:'17'}:fresh?{manual:'12',threshold:'13',overflow:'14'}:{manual:'10',threshold:'07',overflow:'08'};
const report={campaigns:{},source_build_link:{},status:fresh?'PASS_BEHAVIOR_VISUAL_DIFFERENCES_REMAIN':'PASS_WITH_RECORDED_PRODUCTION_GAP'};
const built=read(path.join(base,`compaction20260927-${runs.overflow}/capture.lock.json`));
const manual=read(path.join(base,`compaction20260927-${runs.manual}/capture.lock.json`));
assert.deepEqual(manual.oc.build_command,['cargo','build','--locked']);
const builtManifest=read(path.join(base,`compaction20260927-${runs.overflow}/source-manifest.json`));
const manualManifest=read(path.join(base,`compaction20260927-${runs.manual}/source-manifest.json`));
for(const [file,digest] of Object.entries(builtManifest).filter(([p])=>!p.startsWith('scripts/')))assert.equal(manualManifest[file],digest,file);
report.source_build_link={overflow_build_attempt:runs.overflow,build_command:manual.oc.build_command,native_sha256:manual.oc.executable_sha256,manual_attempt:runs.manual,production_source_inputs_equal:true};
if(fresh) {
  const old=read(path.join(base,'compaction20260927-07/capture.lock.json')), current=read(path.join(base,`compaction20260927-${runs.threshold}/capture.lock.json`));
  const baseline=read(path.join(base,'compaction20260927-10/capture.lock.json'));
  for(const p of ['compaction_fixture.py','bridge.py'])assert.equal(baseline.runner_hashes[p],current.runner_hashes[p]);
  report.fixture_identity={historical_07:old.runner_hashes,current:current.runner_hashes,byte_identical_to_10:true,byte_identical_to_07:false,qualification:'Same threshold policy and emitted seed text/usage; later read-only snapshot coverage and nonempty workspace sentinel already present in 10.'};
  for(const origin of ['upstream','oc']) {
    const oldP=read(path.join(base,'compaction20260927-07',origin,'protocol.json')), newP=read(path.join(base,`compaction20260927-${runs.threshold}`,origin,'protocol.json'));
    const completed=p=>p.filter(e=>e.kind==='provider_completed'&&e.operation==='transcript').slice(0,3).map(e=>e.usage);
    assert.deepEqual(completed(oldP),completed(newP));
    const archive=p=>p.filter(e=>e.kind==='provider'&&e.operation==='transcript').slice(0,3).map(e=>e.request.input.filter(x=>x.role==='user'||x.role==='assistant').map(x=>({role:x.role,content:x.content})));
    assert.deepEqual(archive(oldP),archive(newP));
  }
  const thresholdManifest=read(path.join(base,`compaction20260927-${runs.threshold}/source-manifest.json`));
  for(const [file,digest] of Object.entries(manualManifest).filter(([p])=>!p.startsWith('scripts/')))assert.equal(thresholdManifest[file],digest,file);
  assert.equal(current.oc.executable_sha256,manual.oc.executable_sha256);assert.equal(built.oc.executable_sha256,manual.oc.executable_sha256);
  report.threshold_seed_requests_and_usage_unchanged_from_07=true;
}
const bodyText=b=>JSON.stringify(b);
const data=(s,t)=>s.observations.flatMap(o=>o.data[t]||[]);
const raw=s=>data(s,'messages').length?data(s,'messages'):data(s,'session_message').filter(r=>['user','assistant'].includes(r.type));
const cp=s=>data(s,'session_checkpoint').length?data(s,'session_checkpoint'):data(s,'session_message').filter(r=>r.type==='compaction'&&JSON.parse(r.data).status==='completed');
for(const [mode,suffix] of [...Object.entries(runs),...(optional?[['threshold','21'],['threshold','22']]:[])]) {
  const dir=path.join(base,'compaction20260927-'+suffix), lock=read(path.join(dir,'capture.lock.json'));
  assert.equal(createHash('sha256').update(canonical(read(path.join(dir,'source-manifest.json')))).digest('hex'),lock.oc.source_manifest_sha256);
  assert(read(path.join(dir,'commands.json')).some(c=>c.argv.join(' ')==='cargo build --locked'&&c.exit_code===0));
  if(optional&&['21','22'].includes(suffix)) {
    assert.equal(lock.oc.executable_sha256,manual.oc.executable_sha256);
    const source=read(path.join(dir,'source-manifest.json'));
    for(const [file,digest] of Object.entries(manualManifest).filter(([p])=>!p.startsWith('scripts/')))assert.equal(source[file],digest,file);
    assert.equal(lock.runner_hashes['compaction_fixture.py'],manual.runner_hashes['compaction_fixture.py']);
  }
  const campaign={comparisons:lock.attempts.reduce((a,e)=>(a[e.status]=(a[e.status]||0)+1,a),{}),sides:{}};
  for(const c of lock.captures) {
    const stem=path.join(dir,c.path);assert.equal(sha(stem+'.cells.json'),c.cells_sha256);assert.equal(sha(stem+'.png'),c.png_sha256);
    const grid=read(stem+'.cells.json');assert.equal(grid.cells.length,40);assert(grid.cells.every(row=>row.length===120));
    assert.equal(c.status,!fresh&&mode==='threshold'&&c.origin==='oc'&&c.scenario==='compaction-failure'?'FAILED_STATE':'CAPTURED');
  }
  for(const origin of ['upstream','oc']) {
    const protocol=read(path.join(dir,origin,'protocol.json')), checks=read(path.join(dir,origin,'compaction-checks.json'));
    const requests=protocol.filter(e=>e.kind==='provider'), summaries=requests.filter(e=>e.operation==='compaction');
    assert(requests.every(e=>e.valid));assert(requests.length<=24);
    const snapshots=Object.fromEntries(protocol.filter(e=>e.kind==='compaction_snapshot').map(e=>[e.request_id,e]));
    const side={behavior:checks.status,requests:requests.length,summary_requests:summaries.length,tools:protocol.filter(e=>e.kind==='fixture_tool_call').length,
      exits:protocol.filter(e=>e.kind==='exit'),request_shapes:summaries.map(e=>({roles:e.roles,tools:e.registered_tools.length}))};
    if(fresh) {
      const spec=read(path.join(dir,origin,'bridge-spec.json'));
      const configPath=path.join(spec.isolated_root,origin,'home/config/opencode/opencode.json'), config=read(configPath);
      assert.deepEqual(config.compaction,{auto:mode!=='manual',keep:{tokens:0},buffer:20000});
      side.actual_config={path:configPath,sha256:sha(configPath),compaction:config.compaction,model_limit:(config.provider||config.providers).fixture.models['fixture-model-1'].limit,animations:config.animations??null,snapshots:config.snapshots};
      if(final) {
        const cliPath=path.join(spec.isolated_root,origin,'home/config/opencode/cli.json'),cli=read(cliPath);
        side.actual_cli_config={sha256:sha(cliPath),session:cli.session,session_tps_explicit:cli.session?.tps??null,animations:cli.animations??null};
        if(['21','22'].includes(suffix)) {
          assert.equal(cli.session.tps,false);
          assert.equal(origin==='oc'?config.animations:cli.animations,suffix==='22');
          const frames=lock.captures.filter(c=>c.origin===origin).map(c=>fs.readFileSync(path.join(dir,c.path+'.txt'),'utf8'));
          assert(frames.every(t=>!/[\d.]+\s+(?:tps\b|tok\/s)/.test(t)));
          side.explicit_false_tps_footers_absent=true;
          const baseline=read(path.join(base,'compaction20260927-19',origin,'protocol.json'));
          const seeds=p=>p.filter(e=>e.kind==='provider'&&e.operation==='transcript').slice(0,3).map(e=>e.request.input.filter(x=>x.role==='user'||x.role==='assistant').map(x=>({role:x.role,content:x.content})));
          const usage=p=>p.filter(e=>e.kind==='provider_completed'&&e.operation==='transcript').slice(0,3).map(e=>e.usage);
          assert.deepEqual(seeds(protocol),seeds(baseline));assert.deepEqual(usage(protocol),usage(baseline));
          side.seed_requests_and_usage_unchanged_from_19=true;
        }
      }
      if(mode!=='manual')assert.deepEqual(side.actual_config.model_limit,{context:40000,output:2048});
    }
    if(!fresh&&mode==='threshold'&&origin==='oc') {
      assert.equal(checks.status,'FAILED');assert.equal(summaries.length,0);
      assert(protocol.some(e=>e.kind==='provider_completed'&&e.usage?.input_tokens===23000));
      const next=requests.at(-1).request;assert(bodyText(next).includes('ARCHIVE-1-'));assert(!bodyText(next).includes('VIS34-CHECKPOINT'));
      side.production_gap='Usage threshold ignored: real 23000-token anchor followed by uncompact next request.';
    } else {
      assert.equal(checks.status,'PASS');assert.equal(summaries.length,mode==='manual'?3:1);
      const completed=snapshots[mode==='manual'?'completed':mode+'-completed'];assert(cp(completed).length===1);
      assert(raw(snapshots.before).every(r=>raw(completed).some(a=>JSON.stringify(a)===JSON.stringify(r))));
      assert.deepEqual(completed.project_files,snapshots.before.project_files);
      for(const e of requests.filter(e=>e.operation==='transcript')) {
        const items=e.request.input, calls=new Set(items.filter(x=>x.type==='function_call').map(x=>x.call_id));
        assert(items.filter(x=>x.type==='function_call_output').every(x=>calls.has(x.call_id)));
      }
      const usage=protocol.find(e=>e.kind==='provider_completed'&&e.operation==='compaction').usage;
      assert.equal(usage.input_tokens,1234);assert.equal(usage.output_tokens,321);side.summary_usage=usage;
      if(mode==='manual') {
        assert(Object.keys(completed.project_files).includes('vis34-owner-approved.txt'));
        const queued=snapshots['queued-coalesced'],running=snapshots.running;
        if(origin==='oc') {
          assert.equal(data(queued,'session_compactions').length,1);assert.equal(JSON.parse(data(queued,'session_compactions')[0].snapshot).state,'queued');
          assert.equal(data(queued,'tool_operations')[0].state,'started');assert.equal(data(running,'tool_operations')[0].state,'completed');
        } else {
          assert.equal(data(queued,'session_inbox').length,1);
          const toolState=s=>data(s,'session_message').filter(r=>r.type==='assistant').flatMap(r=>JSON.parse(r.data).content||[]).find(p=>p.id==='call_vis34_hold').state.status;
          assert.equal(toolState(queued),'running');assert.equal(toolState(running),'completed');
        }
        for(const stage of ['failed','cancelled','reopened'])assert.deepEqual(cp(snapshots[stage]),cp(snapshots['checkpoint-before-failure']));
        for(const stage of ['failed','cancelled','reopened'])assert.deepEqual(snapshots[stage].project_files,snapshots['checkpoint-before-failure'].project_files);
        for(const stage of ['failed','cancelled','reopened'])assert(raw(snapshots['checkpoint-before-failure']).every(r=>raw(snapshots[stage]).some(a=>JSON.stringify(a)===JSON.stringify(r))));
        assert.equal(side.tools,1);assert.equal(side.exits.length,2);assert(side.exits.every(e=>e.code===0&&e.termination==='natural'));
        for(const key of ['next-context','restart-context']) {
          const r=checks.checks.find(e=>e.stage===key).request;assert(bodyText(r).includes('VIS34-CHECKPOINT'));assert(!bodyText(r).includes('ARCHIVE-1-'));
          assert(bodyText(r).includes('VIS34 held tool:'));assert(bodyText(r).includes('VIS34 next:'));
          if(key==='restart-context')assert(bodyText(r).includes('VIS34 restart:'));
        }
        assert(fs.readFileSync(path.join(dir,origin,'compaction-completed.txt'),'utf8').includes('Compaction · 1.2K in · 321 out'));
        side.safe_boundary_and_coalescing=true;side.failed_cancelled_restart_checkpoint_unchanged=true;side.nonempty_filesystem_preserved=true;
      } else {
        assert.equal(protocol.filter(e=>e.kind==='provider_overflow').length,mode==='overflow'?1:0);
        assert.equal(side.exits.length,fresh?2:1);assert(side.exits.every(e=>e.code===0&&e.termination==='natural'));
      }
      if(fresh) {
        const undo=checks.checks.find(e=>e.stage==='undo-redo-invariants');assert(undo.provider_requests_unchanged&&undo.raw_history_preserved&&undo.filesystem_preserved);
        const restart=checks.checks.find(e=>e.stage==='restart-context').request;
        assert(bodyText(restart).includes('VIS34-CHECKPOINT'));assert(!bodyText(restart).includes('ARCHIVE-1-'));assert(bodyText(restart).includes('VIS34 restart:'));
        assert.deepEqual(cp(snapshots.reopened),cp(completed));
        if(mode==='manual')assert(checks.checks.find(e=>e.stage==='palette-ack-dismissed').dismissed);
        else assert.equal(side.tools,0);
        if(mode==='threshold') {
          const anchor=protocol.find(e=>e.kind==='provider_completed'&&e.usage?.input_tokens===23000);assert(anchor);assert.equal(anchor.usage.output_tokens,1800);
          assert(summaries[0].index>anchor.index);const next=requests.find(e=>e.index>summaries[0].index&&e.operation==='transcript');assert(next);
          assert(bodyText(next.request).includes('VIS34-CHECKPOINT'));assert(!bodyText(next.request).includes('ARCHIVE-1-'));assert(bodyText(next.request).includes('VIS34 next:'));
          side.usage_anchor=anchor.usage;side.summary_before_next_main=true;
        }
        side.undo_redo_no_provider_or_tool_replay=true;side.restart_checkpoint_and_context=true;
        if(final) {
          const runningStage=mode==='manual'?'running':mode+'-running';
          const text=fs.readFileSync(path.join(dir,origin,`compaction-${mode==='manual'?'running-1':mode+'-running'}.txt`),'utf8');
          assert((suffix==='22'?/[\u2800-\u28ff] Compaction/u.test(text):text.includes('⋯ Compaction'))&&text.includes('VIS34-CHECKPOINT'));
          side.running_observation=suffix==='22'?'Actual live Braille samples/raw PTY timeline; full-grid/PNG captured while owned child paused, phases unaligned.':'Actual held summary stream renders partial Markdown and ellipsis; no FPS/cadence measurement.';
          if(suffix==='22') {
            const observation=checks.checks.find(c=>c.stage==='animation-observation');assert(observation.phases.length>=2);
            const samples=read(path.join(dir,origin,'compaction-animation-samples.json')).samples;
            assert(samples.length===observation.samples&&samples.every(s=>s.text.includes('Compaction')));
            const timeline=fs.readFileSync(path.join(dir,origin,'output-timeline.jsonl'),'utf8').trim().split('\n').map(JSON.parse);
            assert(timeline.length>0&&timeline.every(e=>Number.isFinite(e.at_ns)&&e.bytes===Buffer.from(e.base64,'base64').length));
            assert(protocol.some(e=>e.kind==='scanner_pause_ack'&&e.paused)&&protocol.some(e=>e.kind==='scanner_resume_ack'&&!e.paused));
            side.live_animation=observation;
          }
          if(mode!=='manual') {
            const users=raw(snapshots[runningStage]).filter(r=>r.role==='user'||r.type==='user');
            assert(users.some(r=>JSON.stringify(r).includes('VIS34 next:')));
            if(origin==='oc') {
              const operation=JSON.parse(data(snapshots[runningStage],'session_compactions')[0].snapshot);
              const user=users.find(r=>JSON.stringify(r).includes('VIS34 next:'));
              assert.equal(operation.anchor.message,user.id);assert.equal(operation.state,'running');
            }
            side.current_user_durable_during_running=true;
          } else if(origin==='oc')assert(fs.readFileSync(path.join(dir,origin,'compaction-failed.txt'),'utf8').includes('HTTP status 400: VIS34 bounded summary failure'));
          side.admission_before_accept_and_schema_floor='Not exercised by this admitted fixture; nearest owner tests required.';
        }
      }
    }
    campaign.sides[origin]=side;
  }
  report.campaigns[['21','22'].includes(suffix)?mode+(suffix==='21'?'-tps-false':'-animated'):mode]=campaign;
}
// This check writes a derivative report, never modifies locks, protocol or DBs.
fs.writeFileSync(path.join(base,optional?'compaction-validation-latest-options.json':latest?'compaction-validation-latest.json':final?'compaction-validation-final.json':fresh?'compaction-validation-fresh.json':'compaction-validation.json'),JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify(process.argv.includes('--quiet')?{status:report.status,source_build_link:report.source_build_link,campaigns:Object.fromEntries(Object.entries(report.campaigns).map(([mode,c])=>[mode,{comparisons:c.comparisons,sides:Object.fromEntries(Object.entries(c.sides).map(([side,s])=>[side,{behavior:s.behavior,requests:s.requests,summary_requests:s.summary_requests,tools:s.tools}]))}]))}:report,null,2));
