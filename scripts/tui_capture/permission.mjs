// Real PTY replies only. Independent effects and provider context are preserved.
import fs from 'node:fs';
import path from 'node:path';
export async function probePermission({origin,dir,send,waitFor,frame,capture,visibleMatches,logs,control,relaunch,mode='prompt'}) {
  const result={origin,status:'IN_PROGRESS',checks:[],gaps:['Full styled-grid/PNG parity remains open.','Complete action-by-width matrix, syscall audit, atomic grant failure and release checks remain open.']};
  const save=()=>fs.writeFileSync(path.join(dir,'permission-checks.json'),JSON.stringify(result,null,2)+'\n');
  const record=(stage,data={})=>{result.checks.push({stage,...data});save();};
  const poll=async(predicate,label,timeout=8000)=>{const deadline=performance.now()+timeout;while(performance.now()<deadline){if(predicate())return;await new Promise(r=>setTimeout(r,20));}throw Error('Timed out '+label);};
  const snap=async stage=>{control({kind:'permission_snapshot',request_id:stage});await poll(()=>logs.some(e=>e.kind==='permission_snapshot'&&e.request_id===stage),'snapshot '+stage);const s=logs.find(e=>e.kind==='permission_snapshot'&&e.request_id===stage);record(stage,{snapshot:s});return s;};
  const shot=async(stage,predicate)=>{const f=await waitFor(predicate,stage,18000);record(stage,{capture_status:await capture('permission-'+stage,f,'CAPTURED'),cursor:f.cursor});return f;};
  const requests=()=>logs.filter(e=>e.kind==='provider').length;
  const cleanExit=async generation=>{send('\x03','vis36_final_natural_exit');await poll(()=>logs.some(e=>e.kind==='exit'&&e.generation===generation),'final natural exit',18000);const exit=logs.find(e=>e.kind==='exit'&&e.generation===generation);record('natural-exit-proof',{exit});if(exit.code!==0)throw Error('Nonzero natural exit');};
  const submit=name=>{send('\x1b[200~'+`VIS36 ${name}: execute the supplied ordinary function patch once.`+'\x1b[201~','vis36_prompt');send('\r','vis36_submit');};
  const bytes=(s,text)=>s.files['approval.txt'].bytes_hex===Buffer.from(text+'\n').toString('hex');
  const grants=s=>s.observations.flatMap(o=>o.data.permission_grants||o.data.permission||[]);
  const ops=s=>s.observations.flatMap(o=>o.data.tool_operations||[]);
  const ask=f=>f.text.includes('Permission required')&&f.text.includes('Allow once');
  const done=name=>f=>f.text.includes('VIS36-DONE-'+name)&&!f.text.includes('Permission required');
  // Read the actual visible editor rows; the confirm footer can share its first
  // row. This predicate checks the entire draft, including every wrapped word.
  // Captured grids/PNGs retain the footer and all decoration without masks.
  const visibleFeedback=f=>{const start=visibleMatches(f,'VIS36-FEEDBACK:')[0];if(!start)return '';const lines=[];for(let y=start.y;y<f.rows;y++){const footer=visibleMatches(f,'enter confirm').find(p=>p.y===y);const line=f.cells[y].slice(start.x,footer?.x??f.columns).map(c=>c.symbol).join('').trim();if(line)lines.push(line);}return lines.join(' ').replace(/\s+/g,' ');};
  let previous;
  try {
    await shot('home',f=>f.text.includes('Ask anything'));previous=await snap('seed');
    if(mode!=='prompt') {
      for(const name of ['once','shell','mixed']) {
        submit(name);await shot('auto-'+name+'-completed',done(name));const after=await snap('auto-'+name+'-effects');
        record('auto-'+name+'-proof',{mode,grants:grants(after),files_unchanged:JSON.stringify(previous.files)===JSON.stringify(after.files),actual_results:logs.filter(e=>e.kind==='provider'&&e.case===name).flatMap(e=>e.actual_results||[])});
        if(grants(after).length||(name==='once'&&bytes(after,'before approval'))||(name==='shell'&&!after.files['shell-marker'].present)||(name==='mixed'&&JSON.stringify(previous.files)!==JSON.stringify(after.files)))throw Error('Auto mode effects or grant failure');previous=after;
      }
      await cleanExit(0);result.status='PASS';record('counts',{provider_requests:requests(),provider_completed:logs.filter(e=>e.kind==='provider_completed').length,tool_calls:logs.filter(e=>e.kind==='fixture_tool_call').length,invalid_requests:logs.filter(e=>e.kind==='provider'&&!e.valid).length});return result;
    }
    for(const name of ['once','reject','always']) {
      submit(name);await shot(name+'-ask',ask);
      const pending=await snap(name+'-pending'),count=requests();
      await new Promise(r=>setTimeout(r,500));const pending2=await snap(name+'-held');
      const proof={files_unchanged:JSON.stringify(previous.files)===JSON.stringify(pending.files)&&JSON.stringify(pending.files)===JSON.stringify(pending2.files),no_extra_provider_requests:requests()===count,no_started:!ops(pending).some(o=>String(o.op_id||o.id).endsWith('call_vis36_'+name)),grants:grants(pending)};
      record(name+'-pause-proof',proof);if(!proof.files_unchanged||!proof.no_extra_provider_requests||!proof.no_started)throw Error('Ask did not hold actual effects');
      if(name==='once') {
        const before=await frame(),add=visibleMatches(before,' + ').filter(p=>p.y===0);
        if(add.length!==1)throw Error('New Home add target not unique');
        const click=p=>send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'vis36_approval_tab_mouse');
        record('once-newhome-mouse-target',add[0]);click({...add[0],x:add[0].x+1});
        const home=await shot('once-newhome',f=>f.text.includes('Ask anything')&&!ask(f));
        const away=await snap('once-newhome-held'),attention=visibleMatches(home,'!').filter(p=>p.y===0);
        const homeProof={files_unchanged:JSON.stringify(pending2.files)===JSON.stringify(away.files),no_extra_provider_requests:requests()===count,attention};
        record('once-newhome-pause-proof',homeProof);if(!homeProof.files_unchanged||!homeProof.no_extra_provider_requests||attention.length!==1)throw Error('Home switch lost pending attention or pause');
        click(attention[0]);await shot('once-returned-ask',ask);
      }
      if(name==='once') {send('\x06','vis36_fullscreen');await shot('once-fullscreen',f=>ask(f)&&f.text.includes('minimize'));send('\x06','vis36_inline');const f=await shot('once-inline',f=>ask(f)&&f.text.includes('fullscreen'));if(f.columns!==120){const targets=visibleMatches(f,'Allow once');if(targets.length!==1)throw Error('Once mouse target not unique');const p=targets[0];record('once-mouse-target',p);send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'vis36_once_mouse');}else send('\r','vis36_once');}
      if(name==='reject') {const f=await frame();if(f.columns===80){const p=visibleMatches(f,'Reject').at(-1);record('reject-mouse-target',p);send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'vis36_reject_mouse');}else send('\x1b','vis36_reject');}
      if(name==='always') {send('\x1b[C','vis36_always_select');const f=await shot('always-selected',ask);if(f.columns===80){const p=visibleMatches(f,'Always allow').at(-1);record('always-mouse-target',p);send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'vis36_always_mouse');}else send('\r','vis36_always');}
      await shot(name+'-completed',name==='reject'?f=>!ask(f)&&f.text.includes('interrupted')&&f.text.includes('Patch failed'):done(name));const after=await snap(name+'-effects');
      record(name+'-proof',{files_unchanged:JSON.stringify(previous.files)===JSON.stringify(after.files),grants:grants(after),actual_results:logs.filter(e=>e.kind==='provider'&&e.case===name).flatMap(e=>e.actual_results||[])});
      if(name==='once'&&(bytes(after,'before approval')||grants(after).length))throw Error('Once failed effects or saved grant');
      if(name==='reject'&&JSON.stringify(previous.files)!==JSON.stringify(after.files))throw Error('Reject mutated files');
      if(name==='always'&&!bytes(after,'always approved'))throw Error('Always failed effects');
      previous=after;
    }
    send('\x03','vis36_exit');await poll(()=>logs.some(e=>e.kind==='exit'&&e.generation===0),'clean exit',18000);
    await relaunch();await shot('restart-home',f=>f.text.includes('Ask anything')||f.text.includes('VIS36-DONE'));
    for(const name of ['restart','mixed']) {
      submit(name);await shot(name+'-completed',done(name));const after=await snap(name+'-effects');
      const proof={effect_pass:name==='restart'?bytes(after,'restart approved'):JSON.stringify(previous.files)===JSON.stringify(after.files),grants:grants(after),actual_results:logs.filter(e=>e.kind==='provider'&&e.case===name).flatMap(e=>e.actual_results||[])};
      record(name+'-proof',proof);if(!proof.effect_pass)throw Error(name+' effects mismatch');previous=after;
    }
    for(const name of ['mcp','childroot']) {
      submit(name);await shot(name+'-ask',ask);const pending=await snap(name+'-pending'),count=requests();
      await new Promise(r=>setTimeout(r,500));const held=await snap(name+'-held');
      const proof={files_unchanged:JSON.stringify(previous.files)===JSON.stringify(pending.files)&&JSON.stringify(pending.files)===JSON.stringify(held.files),mcp_call_count:held.mcp.filter(r=>r.method==='tools/call').length,no_extra_provider_requests:requests()===count,no_started:!ops(held).some(o=>String(o.op_id||o.id).endsWith('call_vis36_'+(name==='childroot'?'childread':name)))};
      record(name+'-pause-proof',proof);if(!proof.files_unchanged||!proof.no_extra_provider_requests||!proof.no_started)throw Error(name+' Ask did not hold');
      send('\x06','vis36_extra_fullscreen');await shot(name+'-fullscreen',f=>ask(f)&&f.text.includes('minimize'));send('\x06','vis36_extra_inline');await shot(name+'-inline',f=>ask(f)&&f.text.includes('fullscreen'));
      if(name==='mcp')send('\r','vis36_mcp_once');else {send('\x1b','vis36_child_reject');await shot('child-feedback',f=>f.text.includes('Reject permission'));const feedback='VIS36-FEEDBACK: do not read; continue with this correction.';send('\x1b[200~'+feedback+'\x1b[201~','vis36_child_feedback');const draft=await shot('child-feedback-draft',f=>visibleFeedback(f)===feedback);record('child-feedback-visibility-proof',{expected:feedback,visible:visibleFeedback(draft)});send('\r','vis36_child_feedback_confirm');}
      await shot(name+'-completed',done(name));const after=await snap(name+'-effects');
      const actual=logs.filter(e=>e.kind==='provider'&&e.case===(name==='childroot'?'childread':name)).flatMap(e=>e.actual_results||[]);
      record(name+'-proof',{files_unchanged:JSON.stringify(previous.files)===JSON.stringify(after.files),mcp_call_count:after.mcp.filter(r=>r.method==='tools/call').length,actual_results:actual,provider_requests:requests()-count});previous=after;
    }
    for(const name of ['read','shell','glob','url']) {
      submit(name);
      await shot(name+'-ask',ask);
      const count=requests(),pending=await snap(name+'-pending');
      await new Promise(r=>setTimeout(r,500));const held=await snap(name+'-held');
      const pause={files_unchanged:JSON.stringify(previous.files)===JSON.stringify(pending.files)&&JSON.stringify(pending.files)===JSON.stringify(held.files),no_extra_provider_requests:requests()===count,no_started:!ops(held).some(o=>String(o.op_id||o.id).endsWith('call_vis36_'+name))};
      record(name+'-pause-proof',pause);if(!pause.files_unchanged||!pause.no_extra_provider_requests||!pause.no_started)throw Error(name+' Ask did not hold');
      send('\x06','vis36_preview_fullscreen');await shot(name+'-fullscreen',f=>ask(f)&&f.text.includes('minimize'));send('\x06','vis36_preview_inline');await shot(name+'-inline',f=>ask(f)&&f.text.includes('fullscreen'));
      if(name==='read'||name==='shell')send('\r','vis36_preview_once');else send('\x1b','vis36_preview_reject');
      await shot(name+'-completed',name==='glob'||name==='url'?f=>!ask(f)&&f.text.includes('interrupted'):done(name));const after=await snap(name+'-effects');
      const proof={files_unchanged:JSON.stringify(previous.files)===JSON.stringify(after.files),shell_effect:after.files['shell-marker'].present,no_extra_provider_requests:requests()===count+(name==='read'||name==='shell'?1:0),actual_results:logs.filter(e=>e.kind==='provider'&&e.case===name).flatMap(e=>e.actual_results||[])};
      record(name+'-proof',proof);if(!proof.no_extra_provider_requests||(name==='shell'?!proof.shell_effect:!proof.files_unchanged))throw Error(name+' actual lifecycle failed');previous=after;
    }
    const closeSettings=async(stage)=>{send('\x1b','vis36_settings_escape');await new Promise(r=>setTimeout(r,1000));const f=await frame();record(stage+'-first-escape',{settings_remains:f.text.includes('Settings'),filter_remains:f.text.includes('Permissions')});await capture('permission-'+stage+'-first-escape',f,'CAPTURED');if(f.text.includes('Settings'))send('\x1b','vis36_settings_escape_close');await waitFor(f=>!f.text.includes('Settings'),'settings dismissed',6000);};
    const settings=async(stage)=>{if(stage==='settings-auto-reopen'){send('\x10','vis36_palette');await waitFor(f=>f.text.includes('Commands'),'settings command palette',6000);send('Open settings','vis36_palette_filter');await shot('settings-palette-filtered',f=>f.text.includes('Open settings'));send('\r','vis36_palette_open_settings');}else{send('/settings','vis36_settings');await waitFor(f=>f.text.includes('/settings'),'settings draft',6000);send('\r','vis36_settings_enter');}await shot(stage,f=>f.text.includes('Settings'));};
    await settings('settings-prompt');
    send('Permissions','vis36_settings_filter');await waitFor(f=>f.text.includes('Permissions'),'filtered permissions',6000);send('\r','vis36_settings_toggle');
    await shot('settings-autoaccept',f=>/Autoaccept|auto accept/.test(f.text));await closeSettings('settings-auto');await snap('settings-auto-config');
    const autoRequests=requests(),autoGrants=grants(previous);submit('autoonce');await shot('autoonce-completed',done('autoonce'));const auto=await snap('autoonce-effects');
    record('autoonce-proof',{no_saved_grants:JSON.stringify(autoGrants)===JSON.stringify(grants(auto)),files_unchanged:JSON.stringify(previous.files)===JSON.stringify(auto.files),provider_requests:requests()-autoRequests});previous=auto;
    await settings('settings-auto-reopen');send('Permissions','vis36_settings_filter');await waitFor(f=>f.text.includes('Permissions'),'filtered permissions',6000);send('\r','vis36_settings_toggle');await shot('settings-restored-prompt',f=>/Prompt|prompt/.test(f.text));await closeSettings('settings-prompt');
    submit('promptagain');await shot('promptagain-ask',ask);send('\x1b','vis36_promptagain_reject');await shot('promptagain-rejected',f=>!ask(f)&&f.text.includes('interrupted'));await snap('settings-final-config');
    await cleanExit(1);result.status='PASS';
  }catch(error){result.status='FAILED';result.reason=error.message;record('failure',{reason:error.message});await capture('permission-failure',await frame(),'FAILED_STATE');}
  record('counts',{provider_requests:requests(),provider_completed:logs.filter(e=>e.kind==='provider_completed').length,tool_calls:logs.filter(e=>e.kind==='fixture_tool_call').length,invalid_requests:logs.filter(e=>e.kind==='provider'&&!e.valid).length});save();return result;
}
