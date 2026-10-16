// VIS35 real PTY and independent file-byte/mode evidence. No renderer injection.
import fs from 'node:fs';
import path from 'node:path';
export async function probeApplyPatch({origin,dir,send,waitFor,frame,capture,visibleMatches,logs,control,relaunch}) {
  const result={origin,status:'IN_PROGRESS',checks:[],limitations:['VIS36 accept/reject backend unqualified.','Argument-stream running capture is not a held filesystem executor.','Scripted provider does not qualify A09 real-model patch authorship.']};
  const save=()=>fs.writeFileSync(path.join(dir,'apply-patch-checks.json'),JSON.stringify(result,null,2)+'\n');
  const record=(stage,data={})=>{result.checks.push({stage,...data});save();};
  const poll=async(predicate,label,timeout=6000)=>{const deadline=performance.now()+timeout;while(performance.now()<deadline){if(predicate())return;await new Promise(r=>setTimeout(r,20));}throw Error('Timed out '+label);};
  const snap=async stage=>{control({kind:'patch_snapshot',request_id:stage});await poll(()=>logs.some(e=>e.kind==='patch_snapshot'&&e.request_id===stage),'filesystem '+stage);const value=logs.find(e=>e.kind==='patch_snapshot'&&e.request_id===stage);record(stage,{snapshot:value});return value;};
  const shot=async(stage,predicate)=>{const f=await waitFor(predicate,stage,15000);record(stage,{capture_status:await capture('apply-patch-'+stage,f,'CAPTURED'),cursor:f.cursor});return f;};
  const slash=async command=>{send(command,'vis35_command');await waitFor(f=>f.text.includes(command),'draft '+command,6000);send('\r','vis35_command_enter');};
  const count=()=>logs.filter(e=>e.kind==='fixture_tool_call').length;
  const bytes=(s,name,text)=>s.files[name]?.bytes_hex===Buffer.from(text).toString('hex');
  const pendingLabel='Patch · arguments streaming · no effects yet';
  const occurrences=(text,needle)=>text.split(needle).length-1;
  const durable=s=>s.observations.flatMap(o=>o.data.tool_operations||[]);
  const effects=s=>s.observations.flatMap(o=>o.data.patch_effects||[]);
  const running=f=>(origin==='oc'?f.text.includes(pendingLabel):/Patching|Applying/.test(f.text));
  let previous;
  try {
    await shot('home',f=>f.text.includes('Ask anything'));previous=await snap('seed');
    for(const name of ['create','empty','multihunk','delete','move','replace','multifile','partial','stale','denied']) {
      send('\x1b[200~'+`VIS35 ${name}: execute the supplied ordinary function patch once.`+'\x1b[201~','vis35_prompt');send('\r','vis35_submit');
      await poll(()=>logs.some(e=>e.kind==='fixture_tool_call'&&e.case===name),name+' real call',15000);
      try {
        const deadline=performance.now()+6000;
        let observed=false;
        while(performance.now()<deadline){const f=await frame();if(running(f)&&!f.text.includes('VIS35-DONE-'+name)){observed=true;break;}await new Promise(r=>setTimeout(r,20));}
        if(!observed)throw Error('No running patch presentation in bounded argument-stream interval');
        if(logs.some(e=>e.kind==='fixture_argument_delta'&&e.case===name&&e.phase==='finish'))throw Error('Argument stream finished before pending pause');
        const request_id=name+'-running';control({kind:'pause_scanner',request_id});
        await poll(()=>logs.some(e=>e.kind==='scanner_pause_ack'&&e.request_id===request_id&&e.paused),'running pause ACK');
        try{
          const f=await shot(name+'-running',f=>running(f)&&!f.text.includes('VIS35-DONE-'+name));
          const pending=await snap(name+'-pending');
          const proof={files_unchanged:JSON.stringify(previous.files)===JSON.stringify(pending.files),pending_label_count:occurrences(f.text,pendingLabel),durable_operations_unchanged:JSON.stringify(durable(previous))===JSON.stringify(durable(pending)),effect_rows_unchanged:JSON.stringify(effects(previous))===JSON.stringify(effects(pending)),raw_patch_argument_not_painted:!f.text.includes('*** Begin Patch')};
          record(name+'-pending-proof',proof);
          if(!proof.files_unchanged||(origin==='oc'&&(!proof.durable_operations_unchanged||!proof.effect_rows_unchanged||proof.pending_label_count!==1)))throw Error('Pending phase already mutated files or durable effects / duplicate pending card');
        }
        finally{control({kind:'resume_scanner',request_id});await poll(()=>logs.some(e=>e.kind==='scanner_resume_ack'&&e.request_id===request_id),'running resume ACK');}
      }catch(error){record(name+'-running',{status:'NOT_OBSERVED',reason:error.message});}
      const completed=await shot(name+'-completed',f=>f.text.includes('VIS35-DONE-'+name)&&!f.text.includes(pendingLabel));
      const s=await snap(name+'-effects');
      const operation_rows=durable(s).filter(r=>String(r.op_id||r.id||r.op).endsWith('call_vis35_'+name));
      record(name+'-reconcile',{pending_label_count:occurrences(completed.text,pendingLabel),durable_operation_count:operation_rows.length,durable_operation_rows:operation_rows});
      previous=s;
      const expected={create:()=>bytes(s,'created.ts','const answer = 42;\nconst longValue = "'+'fixture word '.repeat(14)+'";\n'),empty:()=>bytes(s,'empty.txt',''),multihunk:()=>bytes(s,'update.txt',Array.from({length:24},(_,i)=>i===7?'LINE-EIGHT\n':i===19?'LINE-TWENTY\n':`line-${String(i+1).padStart(2,'0')}\n`).join('')),delete:()=>!s.files['delete.txt'].present,move:()=>!s.files['move.txt'].present&&bytes(s,'moved.txt','new move\n'),replace:()=>bytes(s,'replace.txt','replacement\n'),multifile:()=>bytes(s,'multi-a.txt','alpha\n')&&bytes(s,'multi-b.txt','beta\n'),partial:()=>bytes(s,'stale.txt','actual stale\n'),stale:()=>bytes(s,'stale.txt','actual stale\n'),denied:()=>bytes(s,'denied.txt','denied sentinel\n')}[name]();
      record(name+'-proof',{bytes_presence_pass:expected,mode_observations:Object.fromEntries(Object.entries(s.files).filter(([,v])=>v.present).map(([k,v])=>[k,v.mode])),update_mode_preserved:s.files['update.txt'].mode==='0o640',move_mode_preserved:s.files['moved.txt'].present?s.files['moved.txt'].mode==='0o751':null,success_prefix_present:s.files['prefix.txt'].present,actual_model_results:logs.filter(e=>e.kind==='provider'&&e.case===name).flatMap(e=>e.actual_results||[])});
      if(!expected)throw Error(name+' independent bytes/presence mismatch');
      const file={create:'created.ts',empty:'empty.txt',multihunk:'update.txt',delete:'delete.txt',move:'moved.txt',replace:'replace.txt',multifile:'multi-a.txt',partial:'prefix.txt',stale:'stale.txt',denied:'denied.txt'}[name];
      const targets=visibleMatches(completed,file).filter(p=>p.y>0);
      if(targets.length===1){const target=targets[0];send(`\x1b[<35;${target.x+1};${target.y+1}M`,'vis35_hover');await shot(name+'-hover',f=>f.text.includes('VIS35-DONE-'+name));record(name+'-hover-target',{file,target});}
      else record(name+'-hover-target',{file,status:'NOT_UNIQUE_OR_ABSENT',matches:targets.length});
    }
    const before=await snap('before-reopen'),calls=count(),requests=logs.filter(e=>e.kind==='provider').length;
    await slash('/new');await shot('switched',f=>f.text.includes('Ask anything'));
    const reopen=async(stage='reopened')=>{await slash('/sessions');const f=await waitFor(f=>f.text.includes('Sessions')&&f.text.includes('VIS35 patch fixture'),'saved patch session',6000);const matches=visibleMatches(f,'VIS35 patch fixture').filter(p=>p.y>0);if(matches.length!==1)throw Error('Patch session title not unique');const p=matches[0];send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'vis35_reopen');await shot(stage,f=>/denied|Denied|VIS35-DONE/.test(f.text));};
    await reopen();await slash('/undo');await shot('undo',f=>/messages? reverted/.test(f.text));
    send('\x1b[F'+'\x7f'.repeat(160),'vis35_clear_draft');await waitFor(f=>!f.cells[f.cursor.y].map(c=>c.symbol).join('').includes('VIS35'),'clear draft',6000);
    await slash('/redo');await shot('redo',f=>!/messages? reverted/.test(f.text)&&/denied|Denied|VIS35-DONE/.test(f.text));
    const after=await snap('after-redo');
    record('replay-invariants',{files_unchanged:JSON.stringify(before.files)===JSON.stringify(after.files),tool_calls_unchanged:count()===calls,provider_requests_unchanged:logs.filter(e=>e.kind==='provider').length===requests});
    send('\x03','vis35_exit');await waitFor(()=>logs.some(e=>e.kind==='exit'&&e.generation===0),'clean exit',15000);
    await relaunch();await waitFor(f=>f.text.includes('Ask anything')||/denied|Denied|VIS35-DONE/.test(f.text),'restart ready',15000);await reopen('restarted');
     const restarted=await snap('restart-effects');record('restart-invariants',{files_unchanged:JSON.stringify(before.files)===JSON.stringify(restarted.files),tool_calls_unchanged:count()===calls,provider_requests_unchanged:logs.filter(e=>e.kind==='provider').length===requests,durable_operations_unchanged:JSON.stringify(durable(before))===JSON.stringify(durable(restarted)),effect_rows_unchanged:JSON.stringify(effects(before))===JSON.stringify(effects(restarted)),pending_label_count:occurrences((await frame()).text,pendingLabel)});
    if(JSON.stringify(before.files)!==JSON.stringify(restarted.files)||count()!==calls||logs.filter(e=>e.kind==='provider').length!==requests)throw Error('Replay/restart changed effects or requests');
    result.status='PASS';
  }catch(error){result.status='FAILED';result.reason=error.message;record('failure',{reason:error.message});await capture('apply-patch-failure',await frame(),'FAILED_STATE');}
  record('counts',{provider_requests:logs.filter(e=>e.kind==='provider').length,provider_completed:logs.filter(e=>e.kind==='provider_completed').length,tool_calls:count(),invalid_requests:logs.filter(e=>e.kind==='provider'&&!e.valid).length});save();return result;
}
