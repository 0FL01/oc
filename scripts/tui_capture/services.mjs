// VIS42 real application service transitions. Full frames are never masked.
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';

export async function probeServices({origin,dir,send,waitFor,frame,capture,logs,control,prompt}) {
  const result={origin,status:'IN_PROGRESS',checks:[],declared_differences:[
    'Native service identities/error wording are sanitized; brief aggregate status links to /settings /mcps.',
    'Native actual live-projection loss uses a compact outside-message indicator (not exercised by this short answer).',
    'Native unchanged-cause reload does not re-alert; pinned OC2 forgets its prior MCP alert during pending.',
  ]};
  const save=()=>fs.writeFileSync(path.join(dir,'services-checks.json'),JSON.stringify(result,null,2)+'\n');
  const record=(stage,data={})=>{result.checks.push({stage,...data});save();};
  const poll=async(predicate,label)=>{const deadline=performance.now()+8000;while(performance.now()<deadline){if(predicate())return;await new Promise(r=>setTimeout(r,20));}throw Error('Timed out '+label);};
  const snapshot=async stage=>{control({kind:'services_snapshot',request_id:stage});await poll(()=>logs.some(e=>e.kind==='services_snapshot'&&e.request_id===stage),stage);const s=logs.find(e=>e.kind==='services_snapshot'&&e.request_id===stage);record(stage,{snapshot:s});return s;};
  const transition=async action=>{control({kind:'services_control',request_id:action,action});await poll(()=>logs.some(e=>e.kind==='services_control_ack'&&e.request_id===action),'fixture '+action);};
  const shot=async(stage,predicate)=>{
    const started=performance.now(),deadline=started+18000;
    // The live initializer deadline keeps running during capture. Observe the
    // actual stage once, then use the stricter acknowledged full-grid freeze;
    // waiting for unrelated toast/caret clocks to settle can cause a timeout.
    for(;;){
      if(predicate(await frame()))break;
      if(performance.now()>=deadline)throw Error('Timed out observed stage '+stage);
      await new Promise(r=>setTimeout(r,200));
    }
    const observed=performance.now();
    control({kind:'pause_scanner',request_id:stage});
    try {
      await poll(()=>logs.some(e=>e.kind==='scanner_pause_ack'&&e.request_id===stage),'owned capture pause '+stage);
      if(!logs.find(e=>e.kind==='scanner_pause_ack'&&e.request_id===stage).paused)throw Error('Application did not pause for '+stage);
      // Capture exactly the post-ACK real grid, not a closest animation frame.
      const f=await frame();
      if(!predicate(f))throw Error('Observed stage changed before owned pause: '+stage);
      const read=f=>createHash('sha256').update(JSON.stringify(f)).digest('hex');
      record(stage,{capture_status:await capture('services-'+stage,f,'CAPTURED',{read,expected:read(f)}),cursor:f.cursor,owned_pause:true,observed_wait_ms:Math.round(observed-started),pause_capture_ms:Math.round(performance.now()-observed)});
      return f;
    }finally{
      control({kind:'resume_scanner',request_id:stage});
      await poll(()=>logs.some(e=>e.kind==='scanner_resume_ack'&&e.request_id===stage),'owned capture resume '+stage);
    }
  };
  const waitPeer=async(stage,predicate)=>{
    const deadline=performance.now()+8000;
    for(let index=0;performance.now()<deadline;index++){
      const s=await snapshot(stage+'-'+index);
      if(predicate(s.snapshot.mcp))return s;
      await new Promise(r=>setTimeout(r,100));
    }
    throw Error('Timed out actual peer '+stage);
  };
  const command=async(name)=>{send('/'+name,'vis42_command');await waitFor(f=>f.text.includes('/'+name),'command draft',6000);send('\r','vis42_command_enter');};
  const clean=f=>{if(f.text.includes('(warning:')||f.text.includes('VIS42_BACKGROUND_CANARY'))throw Error('Background technical diagnostic reached dialogue');};
  const done=f=>f.text.includes('GEOMETRY-SHORT: tool read completed.')&&!f.text.includes('esc interrupt');
  const alert=f=>origin==='oc'?f.text.includes('service issues · /settings /mcps'):f.text.includes('MCP server failed:');
  try {
    // Identical real unsent input, not a mask or seeded placeholder renderer.
    send('\x1b[200~'+prompt+'\x1b[201~','vis42_prompt');
    await shot('pending-home',f=>f.text.includes(prompt)&&f.text.includes('Reader · MiMo-V2.6-Flash Free'));
    send('\r','vis42_submit');
    clean(await shot('pending-completed-answer',done));
    await transition('fail');
    clean(await shot('failure-alert',f=>origin==='oc'?f.text.includes('3 service issues'):alert(f)));
    const failed=await snapshot('failure-peer');
    clean(await shot('failure-completed-answer',f=>done(f)&&!alert(f)));
    await command('mcps');
    await shot('failure-list',f=>f.text.includes('MCP servers')&&f.text.includes('Failed !'));
    send('\r','vis42_details');
    await shot('failure-details',f=>origin==='oc'?f.text.includes('Stage:'):f.text.includes('MCP server:'));
    if(!failed.snapshot.mcp.some(r=>r.method==='initialize'&&r.phase==='failed'))throw Error('Actual MCP failure was not dispatched; retained actual error details');
    send('\x1b','vis42_details_back');
    await shot('failure-back',f=>f.text.includes('MCP servers')&&f.text.includes('Failed !'));
    send('\x1b','vis42_close_list');await waitFor(done,'closed details',6000);
    await command('reload');
    const priorPids=new Set(failed.snapshot.mcp.map(r=>r.pid));
    await waitPeer('unchanged-reload-peer',rows=>rows.some(r=>r.method==='initialize'&&r.phase==='failed'&&!priorPids.has(r.pid)));
    const reloaded=await shot('unchanged-reload',done);
    clean(reloaded);record('unchanged-reload-notification',{service_alert_visible:alert(reloaded)});
    if(origin==='oc'&&alert(reloaded))throw Error('Unchanged native failure re-alerted');
    await transition('recover');await command('reload');
    await waitPeer('recovery-accepted-peer',rows=>rows.some(r=>r.method==='initialize'&&r.phase==='healthy'));
    await command('mcps');
    await shot('recovered-list',f=>f.text.includes('MCP servers')&&f.text.includes('Connected ✓'));
    send('\x1b','vis42_recovery_back');
    clean(await shot('recovered-answer',f=>done(f)&&!f.text.includes('MCP servers')&&!alert(f)));
    send('\x1b[200~Second same-session spacing check?\x1b[201~','vis42_second_prompt');send('\r','vis42_second_submit');
    clean(await shot('second-completed-answer',f=>f.text.includes('GEOMETRY-TURN-TWO: tool read completed.')&&!f.text.includes('esc interrupt')));
    const healthy=await snapshot('recovery-peer');
    if(!healthy.snapshot.mcp.some(r=>r.method==='initialize'&&r.phase==='healthy'))throw Error('Actual MCP recovery was not dispatched');
    record('counts',{provider_requests:logs.filter(e=>e.kind==='provider').length,invalid_requests:logs.filter(e=>e.kind==='provider'&&!e.valid).length});
    if(logs.some(e=>e.kind==='provider'&&!e.valid))throw Error('Invalid ordinary provider request');
  }catch(error){
    // Preserve the genuine first failure and grid; this is not a parity PASS.
    result.status='FAILED';result.reason=error.message;record('failure',{reason:error.message});await capture('services-failure',await frame(),'FAILED_STATE');save();return result;
  }
  result.status='OBSERVED';save();return result;
}
