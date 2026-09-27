// Opt-in VIS34 actual PTY lifecycle; all requests are emitted by the binaries.
import fs from 'node:fs';
import path from 'node:path';

export async function probeCompaction({origin,dir,send,waitFor,frame,capture,visibleMatches,logs,control,relaunch,trigger='manual',animated=false}) {
  const result={origin,status:'IN_PROGRESS',checks:[],limitations:['No registered production provider-native capability; not exercised.','Automatic threshold/overflow and animated Braille cadence require separate qualification.']};
  const save=()=>fs.writeFileSync(path.join(dir,'compaction-checks.json'),JSON.stringify(result,null,2)+'\n');
  const record=(stage,details={})=>{result.checks.push({stage,...details});save();};
  const summaries=()=>logs.filter(e=>e.kind==='provider'&&e.operation==='compaction');
  const pollLive=async(predicate,label,timeout=6000)=>{const deadline=performance.now()+timeout;while(performance.now()<deadline){const f=await frame();if(predicate(f))return f;await new Promise(r=>setTimeout(r,20));}throw Error('Timed out live observing '+label);};
  const shot=async(stage,predicate)=>{
    if(animated&&stage===trigger+'-running') {
      await pollLive(predicate,stage);
      const samples=[],start=performance.now();
      while(performance.now()-start<1600) {
        const f=await frame(),row=f.cells.findIndex(r=>r.map(c=>c.symbol).join('').includes('Compaction'));
        samples.push({observed_ms:performance.now(),row_1based:row+1,cells:row>=0?f.cells[row]:[],text:row>=0?f.cells[row].map(c=>c.symbol).join(''):''});
        await new Promise(r=>setTimeout(r,20));
      }
      fs.writeFileSync(path.join(dir,'compaction-animation-samples.json'),JSON.stringify({clock:'actual capture-process performance.now; raw PTY timestamps are bridge monotonic_ns',samples},null,2)+'\n');
      record('animation-observation',{samples:samples.length,window_ms:samples.at(-1).observed_ms-samples[0].observed_ms,phases:[...new Set(samples.map(s=>s.text.match(/[\u2800-\u28ff]/u)?.[0]).filter(Boolean))],qualification:'Live observed phases, not phase-aligned across binaries or synthetic 80ms timestamps.'});
      const request_id='compaction-running-pause';control({kind:'pause_scanner',request_id});
      await pollLive(()=>logs.some(e=>e.kind==='scanner_pause_ack'&&e.request_id===request_id&&e.paused),'owned child pause ACK');
      try {
        const f=await waitFor(predicate,stage+' paused',6000),status=await capture('compaction-'+stage,f,'CAPTURED');record(stage,{capture_status:status,owned_child_paused:true});return f;
      } finally {
        control({kind:'resume_scanner',request_id});await pollLive(()=>logs.some(e=>e.kind==='scanner_resume_ack'&&e.request_id===request_id),'owned child resume ACK');
      }
    }
    const f=await waitFor(predicate,stage,15000);const status=await capture('compaction-'+stage,f,'CAPTURED');record(stage,{capture_status:status});return f;
  };
  const submit=async text=>{send('\x1b[200~'+text+'\x1b[201~','vis34_paste');send('\r','vis34_submit');};
  const slash=async()=>{send('/compact','vis34_slash');await waitFor(f=>f.text.includes('/compact'),'compact draft',6000);send('\r','vis34_slash_enter');};
  const snap=async stage=>{control({kind:'compaction_snapshot',request_id:stage});await waitFor(()=>logs.some(e=>e.kind==='compaction_snapshot'&&e.request_id===stage),'DB '+stage,6000);
    const s=logs.find(e=>e.kind==='compaction_snapshot'&&e.request_id===stage);record(stage,{snapshot:s});return s;};
  const setting=async(behavior,release=false)=>{const request_id=`${behavior}-${result.checks.length}`;control({kind:'compaction_control',request_id,behavior,release});await waitFor(()=>logs.some(e=>e.kind==='compaction_control_ack'&&e.request_id===request_id),'fixture control',6000);};
  const checkpoint=s=>s.observations.flatMap(o=>o.data.session_checkpoint||o.data.session_provider_context||
    (o.data.session_message||[]).filter(r=>r.type==='compaction'&&JSON.parse(r.data).status==='completed'));
  const raw=s=>s.observations.flatMap(o=>o.data.messages||o.data.session_message||[]);
  const pairs=request=>{const items=request.input||[];const calls=new Set(items.filter(x=>x.type==='function_call').map(x=>x.call_id));return items.filter(x=>x.type==='function_call_output').every(x=>calls.has(x.call_id));};
  const reopen=async()=>{
    await relaunch();await waitFor(f=>f.text.includes('MiMo'),'restart ready',15000);
    if(!(await frame()).text.includes('VIS34-NEXT-DONE')) {
      send('/sessions','vis34_sessions');await waitFor(f=>f.text.includes('/sessions'),'sessions draft',6000);send('\r','vis34_sessions_enter');
      const f=await waitFor(f=>f.text.includes('Sessions')&&f.text.includes('VIS34 compaction fixture'),'saved session',6000);
      const targets=visibleMatches(f,'VIS34 compaction fixture').filter(p=>p.y>0);if(targets.length!==1)throw Error('Session title not unique');
      const p=targets[0];send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'vis34_reopen');
    }
    await shot('reopened',f=>f.text.includes('VIS34-CHECKPOINT'));
  };
  const undoRedo=async()=>{
    const before=await snap('before-undo'), requests=logs.filter(e=>e.kind==='provider').length;
    send('/undo','vis34_undo');await waitFor(f=>f.text.includes('/undo'),'undo draft',6000);send('\r','vis34_undo_enter');
    await shot('undo',f=>/messages? reverted/.test(f.text)&&!f.text.includes('VIS34-RESTART-DONE'));
    await snap('undo');
    send('\x1b[F'+'\x7f'.repeat(160),'vis34_clear_restored_draft');
    await waitFor(f=>!f.cells[f.cursor.y].map(c=>c.symbol).join('').includes('VIS34'),'clear restored prompt',6000);
    send('/redo','vis34_redo');await waitFor(f=>f.text.includes('/redo'),'redo draft',6000);send('\r','vis34_redo_enter');
    await shot('redo',f=>f.text.includes('VIS34-RESTART-DONE')&&!/messages? reverted/.test(f.text));
    const after=await snap('redo');
    if(JSON.stringify(before.project_files)!==JSON.stringify(after.project_files))throw Error('Undo/Redo mutated workspace');
    if(raw(before).some(r=>!raw(after).some(a=>JSON.stringify(a)===JSON.stringify(r))))throw Error('Undo/Redo mutated raw history');
    if(logs.filter(e=>e.kind==='provider').length!==requests)throw Error('Undo/Redo called provider');
    record('undo-redo-invariants',{provider_requests_unchanged:true,raw_history_preserved:true,filesystem_preserved:true,tool_calls:logs.filter(e=>e.kind==='fixture_tool_call').length});
  };
  try {
    await shot('home',f=>f.text.includes('Ask anything'));
    for(let i=1;i<=3;i++) {
      await submit(`VIS34 seed ${i}: preserve requirement R${i}; no filesystem changes.`);
      await shot('seed-'+i,f=>f.text.includes(`ARCHIVE-${i}-119`)&&logs.some(e=>e.kind==='provider_completed'&&e.operation==='transcript'&&e.index===logs.filter(x=>x.kind==='provider'&&x.operation==='transcript').at(-1)?.index));
    }
    const before=await snap('before');
    if(trigger!=='manual') {
      result.trigger=trigger;result.limitations=['No registered production provider-native capability; not exercised.',animated?'Live Braille/PTY observations only; not phase-aligned or an exact cadence/FPS gate.':'Animated Braille cadence is not qualified.'];save();
      await submit('VIS34 next: continue from checkpoint; no tools.');
      await shot(trigger+'-running',f=>f.text.includes('VIS34-CHECKPOINT')&&summaries().length===1);
      await snap(trigger+'-running');await setting('complete',true);
      await shot(trigger+'-completed',f=>f.text.includes('Continue the user request')&&f.text.includes('VIS34-NEXT-DONE'));
      const after=await snap(trigger+'-completed');
      const request=logs.filter(e=>e.kind==='provider'&&e.operation==='transcript').at(-1).request;
      const serialized=JSON.stringify(request);
      if(!serialized.includes('VIS34-CHECKPOINT')||serialized.includes('ARCHIVE-1-')||!pairs(request))throw Error('Automatic next context invalid');
      if(!checkpoint(after).length)throw Error('No durable automatic checkpoint');
      if(JSON.stringify(before.project_files)!==JSON.stringify(after.project_files))throw Error('Automatic compaction mutated filesystem');
      if(raw(before).some(r=>!raw(after).some(a=>JSON.stringify(a)===JSON.stringify(r))))throw Error('Automatic compaction changed raw history');
      record('automatic-context',{request,overflow_count:logs.filter(e=>e.kind==='provider_overflow').length,summary_count:summaries().length});
      if(summaries().length!==1||logs.filter(e=>e.kind==='provider_overflow').length!==(trigger==='overflow'?1:0))throw Error('Automatic trigger count mismatch');
      send('\x03','vis34_automatic_clean_exit');await waitFor(()=>logs.some(e=>e.kind==='exit'&&e.generation===0),'automatic clean exit',15000);
      await reopen();const reopened=await snap('reopened');
      if(JSON.stringify(checkpoint(after))!==JSON.stringify(checkpoint(reopened)))throw Error('Automatic checkpoint changed on restart');
      await submit('VIS34 restart: continue from persisted checkpoint; no tools.');await shot('restart-next',f=>f.text.includes('VIS34-RESTART-DONE'));
      const restart=logs.filter(e=>e.kind==='provider'&&e.operation==='transcript').at(-1).request;
      if(!JSON.stringify(restart).includes('VIS34-CHECKPOINT')||JSON.stringify(restart).includes('ARCHIVE-1-')||summaries().length!==1)throw Error('Automatic restart context/guard invalid');
      record('restart-context',{request:restart,tool_calls:logs.filter(e=>e.kind==='fixture_tool_call').length});
      await undoRedo();
      send('\x03','vis34_automatic_final_exit');await waitFor(()=>logs.some(e=>e.kind==='exit'&&e.generation===1),'automatic final exit',15000);
      result.status='PASS';save();return result;
    }
    await submit('VIS34 held tool: wait at safe boundary; no filesystem changes.');
    await shot('held-tool',f=>logs.some(e=>e.kind==='fixture_tool_call')&&/sleep|bounded held/.test(f.text));
    await slash();
    await shot('queued',f=>/Compaction|compaction/.test(f.text)&&summaries().length===0);
    send('\x10','vis34_palette');await waitFor(f=>f.text.includes('Commands'),'palette opened',6000);
    send('Compact','vis34_palette_filter');const palette=await shot('palette',f=>visibleMatches(f,'Compact session').length===1);
    const option=visibleMatches(palette,'Compact session')[0];
    send(`\x1b[<0;${option.x+1};${option.y+1}M\x1b[<0;${option.x+1};${option.y+1}m`,'vis34_palette_select');
    await shot('queued-coalesced',f=>/Compaction|compaction/.test(f.text)&&summaries().length===0);
    await snap('queued-coalesced');
    record('palette-ack-dismissed',{dismissed:!(await frame()).text.includes('Commands')});
    if((await frame()).text.includes('Commands')) {record('palette-remained-open',{production_observation:true});send('\x1b','vis34_dismiss_palette');}
    await shot('running-1',f=>f.text.includes('VIS34-CHECKPOINT')&&summaries().length===1);
    await shot('running-2',f=>f.text.includes('VIS34-CHECKPOINT')&&summaries().length===1);
    const running=await snap('running');
    record('safe-boundary',{summary_count:summaries().length,requests:summaries(),running});
    await setting('complete',true);
    await shot('completed',f=>f.text.includes('Continue the user request')&&/1\.2|1,234|1234|1\.4/.test(f.text)&&!f.text.includes('Commands'));
    const completed=await snap('completed');
    if(JSON.stringify(before.project_files)!==JSON.stringify(completed.project_files))throw Error('Compaction mutated workspace');
    const archived=raw(completed);if(raw(before).some(r=>!archived.some(a=>JSON.stringify(a)===JSON.stringify(r))))throw Error('Raw history changed');
    await submit('VIS34 next: continue from checkpoint; no tools.');
    await shot('next',f=>f.text.includes('VIS34-NEXT-DONE'));
    const request=logs.filter(e=>e.kind==='provider'&&e.operation==='transcript').at(-1).request;
    const serialized=JSON.stringify(request);record('next-context',{summary_present:serialized.includes('VIS34-CHECKPOINT'),old_prefix_absent:!serialized.includes('ARCHIVE-1-'),causal_pairs_valid:pairs(request),request});
    if(!serialized.includes('VIS34-CHECKPOINT')||serialized.includes('ARCHIVE-1-')||!pairs(request))throw Error('Next provider context failed checkpoint contract');
    const stable=await snap('checkpoint-before-failure');
    await setting('fail');await slash();
    await shot('failed',f=>/failed|Failed|VIS34 bounded summary failure/.test(f.text));const failed=await snap('failed');
    if(JSON.stringify(checkpoint(stable))!==JSON.stringify(checkpoint(failed)))throw Error('Failed summary installed checkpoint');
    await setting('cancel');const count=summaries().length;await slash();
    await shot('cancel-running',f=>f.text.includes('VIS34-CHECKPOINT')&&summaries().length===count+1&&logs.some(e=>e.kind==='compaction_stream_held'&&e.summary_index===count+1));
    send('\x1b','vis34_cancel');await new Promise(r=>setTimeout(r,300));send('\x1b','vis34_cancel_confirm');
    await shot('cancelled',f=>/cancelled|Cancelled|interrupted|Interrupted/.test(f.text));
    await setting('complete',true);const cancelled=await snap('cancelled');
    if(JSON.stringify(checkpoint(stable))!==JSON.stringify(checkpoint(cancelled)))throw Error('Cancelled summary installed checkpoint');
    send('\x03','vis34_clean_exit');await waitFor(()=>logs.some(e=>e.kind==='exit'&&e.generation===0),'clean exit',15000);
    await reopen();
    const reopened=await snap('reopened');if(JSON.stringify(checkpoint(stable))!==JSON.stringify(checkpoint(reopened)))throw Error('Checkpoint changed on restart');
    await submit('VIS34 restart: continue from persisted checkpoint; no tools.');await shot('restart-next',f=>f.text.includes('VIS34-RESTART-DONE'));
    const restart=logs.filter(e=>e.kind==='provider'&&e.operation==='transcript').at(-1).request;
    if(!JSON.stringify(restart).includes('VIS34-CHECKPOINT')||JSON.stringify(restart).includes('ARCHIVE-1-'))throw Error('Restart provider context invalid');
    record('restart-context',{request:restart,tool_calls:logs.filter(e=>e.kind==='fixture_tool_call').length});
    if(logs.filter(e=>e.kind==='fixture_tool_call').length!==1)throw Error('Tool replayed during continuation/restart');
    await undoRedo();
    send('\x03','vis34_final_exit');await waitFor(()=>logs.some(e=>e.kind==='exit'&&e.generation===1),'final clean exit',15000);
    if(logs.filter(e=>e.kind==='provider').some(e=>!e.valid))throw Error('Invalid fixture requests');
    result.status='PASS';
  } catch(error) {result.status='FAILED';result.reason=error.message;record('failure',{reason:error.message});await capture('compaction-failure',await frame(),'FAILED_STATE');}
  save();return result;
}
