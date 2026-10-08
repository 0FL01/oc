// Actual shared input-history behavior, never a conversation-history substitute.
const FIRST='HISTORY-FIRST Ω界', SECOND='HISTORY-SECOND\nsecond line Ω界', THIRD='HISTORY-THIRD @note.txt done';
const glyphs=row=>(row||[]).filter(c=>c.width!==0).map(c=>c.symbol).join('');
const promptHas=(f,text)=>f.cursor.visible&&glyphs(f.cells[f.cursor.y]).includes(text);
export async function probePromptHistory({origin,mode,send,shot,waitFor,frame,visibleMatches,logs,snapshot,relaunch}) {
  const previous=mode==='remap'?'\x1bOQ':'\x1b[A', next=mode==='remap'?'\x1bOR':'\x1b[B';
  const requests=()=>logs.filter(e=>e.kind==='provider');
  const mains=()=>requests().filter(e=>e.operation==='prompt_history_submit');
  const empty=f=>f.text.includes('Ask anything')||f.cursor.visible&&(f.cells[f.cursor.y]||[]).every(c=>['',' ','┃','│'].includes(c.symbol));
  const clear=async()=>{
    // Forward history exits at the original's empty prompt. Ctrl+C there is a
    // real quit, not a clear; never synthesize a retained draft or revive it.
    if(empty(await frame()))return;
    send('\x03','history_clear_draft');
    return waitFor(empty,'empty editable composer',6000);
  };
  const recall=async(stage,key,first)=>{
    const count=requests().length;send(key,'history_navigation');
    const f=await shot(stage,f=>promptHas(f,first));
    if(requests().length!==count)throw Error('History navigation submitted a request');return f;
  };
  await shot('history-home',f=>f.text.includes('Ask anything'));
  for(const [index,text] of [FIRST,SECOND,THIRD].entries()) {
    send(`\x1b[200~${text}\x1b[201~`,'history_real_prompt');
    await shot('history-input-'+(index+1),f=>f.text.includes(text.split('\n')[0]));
    send('\r','history_explicit_submit');
    await shot('history-accepted-'+(index+1),f=>f.text.includes('VIS-HISTORY-DONE-'+(index+1))&&!f.text.includes('interrupt'));
  }
  await waitFor(()=>requests().length===4,'three main requests and title',15000);
  const accepted=await snapshot('history_three_accepted');
  if(origin==='oc') {
    const data=accepted.observations.map(o=>o.data).find(d=>d.prompt_input_history);
    if(JSON.stringify(data?.prompt_input_history)!==JSON.stringify([FIRST,SECOND,THIRD])||JSON.stringify(data.prompt_user_messages.map(m=>m.text))!==JSON.stringify([FIRST,SECOND,THIRD]))throw Error('Native accepted input list/RAW user rows disagree');
  }
  send('\x1b[200~scratch Ω\x1b[201~','history_unfinished_draft');
  const scratch=await shot('history-unfinished',f=>promptHas(f,'scratch Ω'));
  if(mode==='remap') {
    send('\x1b[A','history_disabled_default');
    const unchanged=await shot('history-remapped-old-up',f=>promptHas(f,'scratch Ω'));
    if(unchanged.text.includes('HISTORY-THIRD')&&promptHas(unchanged,'HISTORY-THIRD'))throw Error('Old Up recalled under remap');
    send('\x1b[F','history_restore_raw_end');await waitFor(f=>promptHas(f,'scratch Ω')&&f.cursor.x===scratch.cursor.x,'raw end after disabled history key',6000);
  }
  const atEdge=await recall('history-unfinished-edge',previous,'scratch Ω');
  if(atEdge.cursor.x>=scratch.cursor.x)throw Error('First previous action skipped raw boundary navigation');
  send(previous,'history_navigation');
  const attempted=await shot('history-session-latest',f=>promptHas(f,'HISTORY-THIRD')||origin==='upstream'&&promptHas(f,'scratch Ω'));
  const reference_nonempty_refused=origin==='upstream'&&promptHas(attempted,'scratch Ω');
  if(reference_nonempty_refused) {
    // The pinned provider's index-zero/current-text guard refuses an arbitrary
    // nonempty draft. Observe that actual frame, then clear only the reference
    // through its real input path to qualify the remaining browse operations.
    await clear();send(previous,'history_reference_empty_browse');await waitFor(f=>promptHas(f,'HISTORY-THIRD'),'reference empty-draft recall',6000);
  }
  await recall('history-session-older',previous,'HISTORY-SECOND');
  send(next,'history_visual_next');
  await shot('history-next-visual',f=>promptHas(f,'second line'));
  send(next,'history_next_raw_edge');
  await shot('history-next-edge',f=>promptHas(f,'second line'));
  await recall('history-next-latest',next,'HISTORY-THIRD');
  send(next,'history_forward_boundary');
  const forward=await shot('history-forward-boundary',f=>origin==='oc'?promptHas(f,'scratch Ω'):f.cursor.visible&&(f.cells[f.cursor.y]||[]).every(c=>['',' ','┃','│'].includes(c.symbol)));
  if(origin==='oc'&&!promptHas(forward,'scratch Ω'))throw Error('Native Down lost pre-browse draft');
  await clear();
  await recall('history-edit-base',previous,'HISTORY-THIRD');send('X','history_edit_copy');
  await shot('history-edited-copy',f=>promptHas(f,'XHISTORY-THIRD'));
  send(previous+previous,'history_edited_previous');
  await shot('history-edited-refuses-older',f=>promptHas(f,'XHISTORY-THIRD'));
  send('\x1b[3~','history_remove_edit');await waitFor(f=>promptHas(f,'HISTORY-THIRD')&&!promptHas(f,'XHISTORY-THIRD'),'restore copied bytes',6000);
  await clear();
  // The actual add-tab control is not a submitted slash input and therefore
  // cannot itself become the newest history item before Home recall.
  const add=(await frame());const hits=visibleMatches(add,'+').filter(p=>p.y===0);
  if(hits.length!==1)throw Error('New Home tab hit is not unique');
  const p=hits[0];send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'history_new_home');
  await shot('history-new-home',f=>f.text.includes('Ask anything'));
  await recall('history-home-shared',previous,'HISTORY-THIRD');
  send('\x10','history_command_overlay');await shot('history-overlay',f=>f.text.includes('Commands'));
  send('\x1b[B','history_overlay_down');await shot('history-overlay-owned',f=>f.text.includes('Commands'));
  send('\x1b','history_close_overlay');await waitFor(f=>!f.text.includes('Commands')&&promptHas(f,'HISTORY-THIRD'),'history draft after overlay',6000);
  if(mode==='remap') {
    send('\x1b[B','history_disabled_next');await shot('history-remapped-old-down',f=>promptHas(f,'HISTORY-THIRD'));
  }
  const beforeSubmit=requests().length;
  if(beforeSubmit!==4)throw Error('Browsing/Home/overlay generated a request');
  send('\r','history_explicit_recalled_submit');
  await shot('history-recalled-accepted',f=>f.text.includes('VIS-HISTORY-DONE-4')&&!f.text.includes('interrupt'));
  await waitFor(()=>requests().length===6,'four main requests plus two titles',15000);
  if(mains().length!==4||requests().some(e=>!e.valid)||JSON.stringify(mains()[3].user_texts)!==JSON.stringify([THIRD]))throw Error('Recalled text was not one exact fresh-session wire input');
  const beforeRestart=await snapshot('history_before_restart');
  if(origin==='oc') {
    const data=beforeRestart.observations.map(o=>o.data).find(d=>d.prompt_input_history);
    if(JSON.stringify(data.prompt_input_history)!==JSON.stringify([FIRST,SECOND,THIRD])||data.prompt_user_messages.length!==4)throw Error('Consecutive recalled acceptance duplicated history');
  }
  send('\x03','history_clean_exit');
  const deadline=Date.now()+6000;while(!logs.some(e=>e.kind==='exit'&&e.generation===0)&&Date.now()<deadline)await new Promise(r=>setTimeout(r,25));
  if(!logs.some(e=>e.kind==='exit'&&e.generation===0))throw Error('History process did not exit cleanly');
  await relaunch();await waitFor(f=>f.text.includes('Ask anything')||f.text.includes('VIS-HISTORY-DONE'),'same-root history restart',15000);
  await recall('history-restarted-shared',previous,'HISTORY-THIRD');
  if(mode==='remap') {
    await clear();send('\x07p','history_effective_leader_previous');
    await shot('history-restarted-leader',f=>promptHas(f,'HISTORY-THIRD'));
  }
  const afterRestart=await snapshot('history_restarted');
  if(requests().length!==6||[accepted,beforeRestart,afterRestart].some(s=>s.mcp_calls.length||s.shell_effect!==null)||JSON.stringify(afterRestart.observations)!==JSON.stringify(beforeRestart.observations))throw Error('Restart/browsing replayed or changed accepted data');
  await clear();send('/mcps','history_accepted_slash');await waitFor(f=>f.text.includes('/mcps'),'slash input',6000);send('\r','history_slash_dispatch');
  await shot('history-slash-accepted',f=>f.text.includes('MCP servers')&&f.text.includes('No items available'));
  const slash=await snapshot('history_slash_recorded');
  if(origin==='oc'&&!slash.observations.some(o=>o.data.prompt_input_history?.at(-1)==='/mcps'))throw Error('Accepted slash input not recorded before its view');
  if(requests().length!==6)throw Error('Slash view dispatched model generation');
  return {status:'OBSERVED_PROMPT_HISTORY',mode,accepted,before_restart:beforeRestart,after_restart:afterRestart,slash,
    explicit_user_submissions:4,provider_requests:6,tool_effects:0,shared_home_session_restart:true,
    native_unfinished_draft_restored:origin==='oc',reference_forward_boundary:'EMPTY_PROMPT',
    reference_nonempty_draft_refused:reference_nonempty_refused,
    native_consecutive_dedup_verified:origin==='oc',native_raw_users_verified:origin==='oc',effective_leader_verified:mode==='remap'};
}
