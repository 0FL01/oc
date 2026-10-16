// Real parent delegation, child-owned Shell and terminal effects, no UI stories.
const glyphs=row=>row.map(c=>c.width===0?'':c.symbol).join('');
const activeTab=(f,label)=>f.cells.some(row=>{
  const text=glyphs(row),x=text.indexOf(label);
  return text.includes('Subagents')&&text.includes('Terminals')&&x>=0&&row[x]?.modifiers.includes('bold');
});
export async function probeComposer(args) {
  if(args.rootHandoff)return probeRootHandoff(args);
  if(args.modelShell)return probeRootModelShell(args);
  if(args.subagentCards)return probeSubagentCards(args);
  const {origin,send,shot,waitFor,frame,logs,snapshot,control}=args;
  const openParent=async()=>{
    if(origin==='oc') {send('\x07','composer_parent_subagents');return;}
    const f=await frame(),y=f.cells.findIndex(row=>glyphs(row).includes('ctrl+g 1 subagent'));
    const x=y<0?-1:glyphs(f.cells[y]).indexOf('ctrl+g 1 subagent');
    if(x<0)throw Error('Actual parent live-status action missing');
    send(`\x1b[<0;${x+1};${y+1}M\x1b[<0;${x+1};${y+1}m`,'composer_parent_live_status_click');
  };
  const requestCount=()=>logs.filter(e=>e.kind==='provider').length;
  const nativeRows=(snapshot,table)=>snapshot.composer.observations.flatMap(o=>o.data[table]||[]);
  const nativeOwnerFacts=snapshot=>{
    const sessions=nativeRows(snapshot,'sessions'),children=nativeRows(snapshot,'child_jobs');
    if(sessions.length!==2||children.length!==1)throw Error('Delegation did not create exactly one linked child');
    const root=sessions.find(s=>s.parent_id===null),child=sessions.find(s=>s.parent_id===root?.id),job=children[0];
    if(!root||!child||job.parent_id!==root.id||job.child_id!==child.id)throw Error('Stored child relationship differs from its admitted owner');
    const identity=JSON.parse(job.identity);
    const shells=nativeRows(snapshot,'shell_jobs').map(row=>({row,provenance:JSON.parse(row.provenance),outcome:row.outcome===null?null:JSON.parse(row.outcome)}));
    if(shells.some(({row,provenance:p})=>row.session_id!==child.id||p.session!==child.id||p.operation!==row.operation_id||p.location!==identity.location||p.generation!==identity.generation||!p.turn))throw Error('Child Shell lost its captured source/Location/generation/turn');
    return {root,child,job,identity,shells};
  };
  const observe=async(label,predicate)=>{
    const deadline=Date.now()+15000;
    while(Date.now()<deadline) {
      const s=await snapshot(label+'-'+Date.now());
      if(predicate(s.composer))return s;
      await new Promise(r=>setTimeout(r,50));
    }
    throw Error('Timed out owner facts '+label);
  };
  const release=async phase=>{
    const id='release-'+phase;
    control({kind:'tool_preview_control',request_id:id,action:id});
    const deadline=Date.now()+6000;
    while(!logs.some(e=>e.kind==='tool_preview_control_ack'&&e.request_id===id)&&Date.now()<deadline)await new Promise(r=>setTimeout(r,25));
    if(!logs.some(e=>e.kind==='tool_preview_control_ack'&&e.request_id===id))throw Error('Missing owned release acknowledgement');
  };
  await shot('composer-home',f=>f.text.includes('Ask anything'));
  send('\x1b[200~VIS39_ROOT_TASK\x1b[201~\r','composer_parent_submit');
  const started=await observe('composer_first_ready',s=>s.ready.first&&s.effects.started.lines.length===1);
  if(started.composer.effects.completed.lines.length!==0)throw Error('First command completed before its held output could be inspected');
  const initial=origin==='oc'?nativeOwnerFacts(started):null;
  if(initial&&(initial.shells.length!==1||initial.shells[0].row.phase!=='running'||initial.shells[0].outcome!==null))throw Error('Native initial output was not a real running child-owned process');
  await openParent();
  await shot('composer-parent-subagents',f=>f.text.includes('Subagents')&&f.text.includes('Inspect child shell')&&f.text.includes('Running'));
  if(origin==='upstream') {
    // Pinned session.background is a base-layer action, not registered in the
    // lower composer's mode. Use its real advertised parent action after close.
    send('\x1b','composer_reference_close_before_background');
    await waitFor(f=>!activeTab(f,'Subagents'),'reference base action scope',6000);
  }
  send('\x02','composer_foreground_child_to_background');
  await waitFor(f=>f.text.includes('VIS39-PARENT-DONE'),'actual parent continued after conversion',15000);
  if(origin==='oc')send('\x1b','composer_parent_close');
  await waitFor(f=>!f.text.includes('show inactive'),'parent composer closed',6000);
  send('\x1b[200~VIS39 parent unsent draft\x1b[201~','composer_parent_draft');
  await shot('composer-parent-draft',f=>f.text.includes('VIS39 parent unsent draft'));
  const requestsBeforeRoute=requestCount();
  const converted=await snapshot('composer_converted_before_route');
  if(converted.composer.effects.started.lines.length!==1||converted.composer.effects.completed.lines.length!==0)throw Error('Background conversion re-executed or prematurely completed the foreground child');
  if(initial) {
    const live=nativeOwnerFacts(converted);
    if(live.job.operation_id!==initial.job.operation_id||live.shells.length!==1||live.shells[0].row.operation_id!==initial.shells[0].row.operation_id||live.shells[0].row.process!==initial.shells[0].row.process)throw Error('Background conversion replaced the original child or process identity');
  }
  await openParent();
  await waitFor(f=>activeTab(f,'Subagents'),'parent family selector',6000);
  send('\r','composer_open_live_child');
  // At 80x24 the real child transcript follows its held Shell output, so the
  // earlier child prompt is offscreen. The next Shell-tab shot independently
  // requires this source session's running command (the parent has none).
  await shot('composer-child-subagents',f=>activeTab(f,'Subagents')&&f.text.includes('Inspect child shell')
    &&(f.text.includes('VIS39_CHILD_TASK')||f.text.includes('VIS39-ROW-079')));
  if(origin==='oc') {
    const childFrame=await frame();
    if(!childFrame.text.split('\n')[0].includes('VIS39 parent'))throw Error('Linked child replaced the retained root-deck title');
    if(childFrame.text.includes('[parent_context_pack]'))throw Error('Child display leaked the native task envelope');
  }
  send('\x1b[C','composer_child_shell_tab');
  await shot('composer-child-shell',f=>activeTab(f,'Shell')&&f.text.includes('python3 composer-probe.py first'));
  send('\r','composer_child_output');
  await shot('composer-output-running',f=>f.text.includes('Shell output')&&f.text.includes('Running')&&f.text.includes('VIS39-ROW-079'));
  send('\x1b[H','composer_output_home');
  await shot('composer-output-home',f=>f.text.includes('Shell output')&&f.text.includes('VIS39-LIVE-first'));
  send('\x1b[6~','composer_output_page_down');
  await shot('composer-output-page',f=>f.text.includes('Shell output')&&!f.text.includes('VIS39-LIVE-first'));
  send('\x1b[F','composer_output_follow');
  await shot('composer-output-follow',f=>f.text.includes('VIS39-ROW-079'));
  await release('first');
  const second=await observe('composer_second_ready',s=>s.ready.second&&s.effects.completed.lines.length===1);
  await shot('composer-output-final',f=>f.text.includes('Shell output')&&f.text.includes('Exited')&&f.text.includes('VIS39-FINAL-FLUSH-first'));
  send('\x1b','composer_close_output_not_job');
  await shot('composer-second-shell',f=>f.text.includes('Subagents')&&f.text.includes('python3 composer-probe.py second')&&!f.text.includes('Shell output'));
  send('\x04','composer_kill_exact_child_shell');
  await observe('composer_killed',s=>s.effects.started.lines.length===2&&s.effects.completed.lines.length===1);
  await waitFor(f=>f.text.includes('VIS39-CHILD-DONE')||f.text.includes('No shell commands'),'child kill settles',15000);
  if(!activeTab(await frame(),'Shell')) {
    // The pinned child route may restore its default Subagents tab on settle.
    // Navigate the actual shared surface back to Shell; immutable transcript
    // commands are not evidence that an execution is still running.
    send('\x1b[C','composer_settled_child_shell_tab');
  }
  await shot('composer-child-empty-shell',f=>f.text.includes('No shell commands'));
  send('\x1b','composer_return_parent');
  await shot('composer-parent-restored',f=>f.text.includes('VIS39 parent unsent draft')&&f.text.includes('↳ Helper finished'));
  const noticeFrame=await frame(),noticeY=noticeFrame.cells.findIndex(row=>glyphs(row).includes('↳ Helper finished'));
  const noticeX=noticeY<0?-1:glyphs(noticeFrame.cells[noticeY]).indexOf('↳ Helper finished');
  if(noticeX<0)throw Error('Typed completion link is absent from the actual parent viewport');
  send(`\x1b[<0;${noticeX+1};${noticeY+1}M\x1b[<0;${noticeX+1};${noticeY+1}m`,'composer_notice_open_original_child');
  await shot('composer-notice-child',f=>activeTab(f,'Subagents')&&f.text.includes('VIS39-CHILD-DONE'));
  send('\x1b','composer_notice_return_parent');
  await shot('composer-notice-parent-restored',f=>f.text.includes('VIS39 parent unsent draft')&&f.text.includes('↳ Helper finished'));
  // The actual terminal selector has no initial row. Enter must not create.
  send('\x18\x1b[B','composer_terminal_selector');
  await shot('composer-terminals-empty',f=>f.text.includes('Terminals')&&f.text.includes('+ New terminal'));
  send('\r','composer_terminal_undefined_enter');
  const undefinedEnter=await snapshot('composer_terminal_undefined_enter');
  if(!activeTab(await frame(),'Terminals'))throw Error('Undefined terminal selection activated an entry');
  if(origin==='oc'&&nativeRows(undefinedEnter,'terminals').length!==0)throw Error('Undefined Enter created a native terminal');
  send('\x1b[B\r','composer_create_selected_terminal');
  await shot('composer-terminal-pane',f=>!f.text.includes('Subagents')&&f.text.includes('VIS39 parent unsent draft'));
  send('\x18\x1b[C','composer_terminal_focus');
  send(`printf '\\033[31mVIS39-TERMINAL\\033[0m\\n'; printf 'terminal\\n' >> composer-${origin}-terminal.effects\r`,'composer_terminal_actual_input');
  const terminal=await observe('composer_terminal_effect',s=>s.effects.terminal.lines.length===1);
  await shot('composer-terminal-effect',f=>f.text.includes('VIS39-TERMINAL'));
  send('\x18\x1b[D','composer_terminal_focus_prompt');
  send('\x18t','composer_terminal_hide');
  await shot('composer-terminal-hidden',f=>f.text.includes('VIS39 parent unsent draft')&&!f.text.includes('VIS39-TERMINAL'));
  send('\x18t','composer_terminal_show_existing');
  await shot('composer-terminal-restored',f=>f.text.includes('VIS39-TERMINAL'));
  const final=await snapshot('composer_final');
  if(final.composer.effects.started.lines.length!==2||final.composer.effects.completed.lines.length!==1||final.composer.effects.terminal.lines.length!==1)throw Error('Composer navigation replayed or retargeted an effect');
  if(logs.some(e=>e.kind==='provider'&&!e.valid)||logs.filter(e=>e.kind==='fixture_tool_call').length!==3)throw Error('Delegation/Shell graph did not execute exactly once');
  if(requestsBeforeRoute!==requestCount()&&requestCount()>12)throw Error('Unbounded provider continuation');
  let ownerFacts=null;
  if(initial) {
    const finished=nativeOwnerFacts(final);
    const first=finished.shells.find(s=>s.row.operation_id===initial.shells[0].row.operation_id);
    const killed=finished.shells.find(s=>s.provenance.command==='python3 composer-probe.py second');
    if(finished.job.operation_id!==initial.job.operation_id||finished.shells.length!==2||first?.outcome?.state!=='completed'||first.outcome.exit!==0||!first.outcome.display_recent?.includes('VIS39-FINAL-FLUSH-first')||killed?.outcome?.state!=='cancelled'||!killed.outcome.cancelled||killed.outcome.timeout)throw Error('Stored terminal Shell facts do not prove original final flush and exact second-job kill');
    const terminals=nativeRows(final,'terminals'),selected=nativeRows(final,'terminal_selection');
    if(terminals.length!==1||terminals[0].session_id!==finished.root.id||terminals[0].live!==1||selected.length!==1||selected[0].session_id!==finished.root.id||selected[0].terminal_id!==terminals[0].id)throw Error('Terminal hide/show changed the captured root terminal identity or live selection');
    ownerFacts={root:finished.root.id,child:finished.child.id,child_operation:finished.job.operation_id,
      same_process_conversion:true,child_shell_source_fenced:true,final_flush_completed:true,
      second_shell_cancelled:true,undefined_terminal_enter_noop:true,same_terminal_hide_show:true,
      typed_child_notice_link:true};
  }
  return {status:'OBSERVED_COMPOSER',started,converted,second,undefinedEnter,terminal,final,owner_facts:ownerFacts,
    origin,actual_tool_calls:3,actual_requests:requestCount(),shell_starts:2,shell_completions:1,
    terminal_effects:1,owner_snapshot_read_only:true,no_effect_replay:true};
}

async function probeSubagentCards({origin,send,shot,waitFor,frame,logs,snapshot,control,resize}) {
  const rows=(s,table)=>s.composer.observations.flatMap(o=>o.data[table]||[]);
  const caption=f=>f.text.includes('Helper Subagent')&&f.text.includes('Inspect captured child shell')&&f.text.includes('Fixture Caption Model');
  const observe=async(label,predicate)=>{
    const deadline=Date.now()+15000;
    while(Date.now()<deadline){const s=await snapshot(label+'-'+Date.now());if(predicate(s.composer))return s;await new Promise(r=>setTimeout(r,50));}
    throw Error('Missing actual Subagent owner '+label);
  };
  const command=async action=>{
    const id='subagent-'+action;
    control({kind:'tool_preview_control',request_id:id,action});
    const deadline=Date.now()+6000;
    while(!logs.some(e=>e.kind==='tool_preview_control_ack'&&e.request_id===id)&&Date.now()<deadline)await new Promise(r=>setTimeout(r,25));
    const ack=logs.find(e=>e.kind==='tool_preview_control_ack'&&e.request_id===id);
    if(!ack)throw Error('No owned Subagent fixture acknowledgement '+action);
    return ack;
  };
  const headerPoint=f=>{
    const y=f.cells.findIndex(row=>glyphs(row).includes('Helper Subagent'));
    const x=y<0?-1:glyphs(f.cells[y]).indexOf('Helper Subagent');
    if(x<0)throw Error('Actual painted inline Subagent header is absent');
    return {x:x+1,y:y+1};
  };
  const hover=async stage=>{
    const before=await frame(),p=headerPoint(before);
    send(`\x1b[<35;${p.x};${p.y}M`,'subagent_card_hover');
    const after=await shot(stage,f=>caption(f)&&f.text.includes(' Background ')&&f.text.includes('VIS39 parent unsent draft'));
    const a=before.cells[p.y-1][p.x-1],b=after.cells[p.y-1][p.x-1];
    if(a.symbol!==b.symbol||a.fg===b.fg)throw Error('Real Subagent hover did not change the painted heading color');
    if(JSON.stringify(before.cursor)!==JSON.stringify(after.cursor))throw Error('Inline hover changed the actual prompt caret');
    return {point:p,before:a.fg,after:b.fg};
  };
  send('\x1b[200~VIS39_ROOT_TASK\x1b[201~\r','subagent_card_submit');
  const started=await observe('first-ready',s=>s.ready.first&&s.effects.started.lines.length===1);
  await shot('subagent-foreground-120',caption);
  if(origin==='oc') {
    send('\x07','subagent_card_native_foreground_scope');
    await waitFor(f=>activeTab(f,'Subagents'),'actual foreground child selector',6000);
  }
  send('\x02','subagent_card_actual_background');
  await waitFor(f=>f.text.includes('VIS39-PARENT-DONE'),'actual parent continuation',15000);
  if(origin==='oc')send('\x1b','subagent_card_close_selector');
  await waitFor(f=>!activeTab(f,'Subagents'),'closed foreground selector',6000);
  send('\x1b[200~VIS39 parent unsent draft\x1b[201~','subagent_card_preserved_draft');
  await shot('subagent-background-120',f=>caption(f)&&f.text.includes(' Background ')&&f.text.includes('VIS39 parent unsent draft'));
  const hoverFacts=[];
  hoverFacts.push(await hover('subagent-hover-120'));
  for(const [columns,height] of [[80,24],[160,48]]) {
    send('\x1b[<35;1;1M','subagent_card_leave_header');
    await resize(height,columns);
    await shot('subagent-background-'+columns,f=>f.columns===columns&&f.rows===height&&caption(f)&&f.text.includes(' Background ')&&f.text.includes('VIS39 parent unsent draft'));
    hoverFacts.push(await hover('subagent-hover-'+columns));
  }
  const p=headerPoint(await frame());
  send(`\x1b[<0;${p.x};${p.y}M\x1b[<0;${p.x};${p.y}m`,'subagent_card_open_captured_child');
  await shot('subagent-card-child',f=>!f.text.includes('VIS39 parent unsent draft')&&(f.text.includes('VIS39_CHILD_TASK')||f.text.includes('VIS39-ROW-079')));
  send('\x1b','subagent_card_return_parent');
  await shot('subagent-card-parent',f=>caption(f)&&f.text.includes('VIS39 parent unsent draft'));
  const converted=await snapshot('subagent_card_converted');
  if(converted.composer.effects.started.lines.length!==1||converted.composer.effects.completed.lines.length!==0)throw Error('Card navigation replayed/completed the held child');
  if(origin==='oc'){
    const a=rows(started,'child_jobs'),b=rows(converted,'child_jobs'),sa=rows(started,'shell_jobs'),sb=rows(converted,'shell_jobs');
    if(a.length!==1||b.length!==1||a[0].identity!==b[0].identity||a[0].operation_id!==b[0].operation_id||sa.length!==1||sb.length!==1||sa[0].process!==sb[0].process||sa[0].provenance!==sb[0].provenance)throw Error('Inline conversion/navigation retargeted original family/process');
  }
  // Finish the same held process, then cancel only its actual second Shell via
  // the existing selected-source UI. No terminal or additional child is seeded.
  await command('release-first');
  await observe('second-ready',s=>s.ready.second&&s.effects.completed.lines.length===1);
  if(origin==='oc')send('\x07','subagent_card_native_child_selector');
  else {
    const f=await frame(),y=f.cells.findIndex(row=>glyphs(row).includes('ctrl+g 1 subagent'));
    const x=y<0?-1:glyphs(f.cells[y]).indexOf('ctrl+g');
    if(x<0)throw Error('Actual original live-child action missing');
    send(`\x1b[<0;${x+1};${y+1}M\x1b[<0;${x+1};${y+1}m`,'subagent_card_reference_child_selector');
  }
  await waitFor(f=>activeTab(f,'Subagents'),'actual live child selector',6000);
  send('\r','subagent_card_enter_original_child');
  await waitFor(f=>activeTab(f,'Subagents')&&f.text.includes('VIS39-ROW-079'),'captured child source',6000);
  send('\x1b[C','subagent_card_source_shell_tab');
  await waitFor(f=>activeTab(f,'Shell')&&f.text.includes('python3 composer-probe.py second'),'selected actual second process',6000);
  send('\x04','subagent_card_cancel_second_only');
  await waitFor(f=>f.text.includes('VIS39-CHILD-DONE'),'actual child settled',15000);
  send('\x1b','subagent_card_settled_parent');
  await resize(40,120);
  await shot('subagent-completed',f=>caption(f)&&f.text.includes('VIS39 parent unsent draft')&&f.text.includes('↳ Helper finished'));
  const settled=await snapshot('subagent_card_settled');
  const capture=await command('capture-continuation');
   send('\x03','subagent_card_clear_draft_for_next_user');
   await waitFor(f=>!f.text.includes('VIS39 parent unsent draft'),'actual parent draft cleared',6000);
  send('\x1b[200~VIS39_CONTINUE_ROOT_TASK\x1b[201~\r','subagent_card_continue_same_child');
  await waitFor(f=>f.text.includes('Continue subagent'),'actual continuation tool part',15000);
  const running=await observe('continuation-running',s=>origin!=='oc'||s.observations.some(o=>(o.data.child_jobs||[]).some(j=>j.state==='running')));
  await shot('subagent-continuation-running',f=>caption(f)&&f.text.includes('Continue subagent')&&!f.text.includes('VIS39-CONTINUED-PARENT-DONE'));
  await command('release-continuation');
  await shot('subagent-continuation-completed',f=>caption(f)&&f.text.includes('Continue subagent')&&f.text.includes('VIS39-CONTINUED-PARENT-DONE'));
   send('\x1b[200~VIS39_MISSING_AGENT_TASK\x1b[201~\r','subagent_card_missing_agent');
   // Native's exact deny policy rejects the missing profile before lookup;
   // pinned original resolves that profile first. Both are real failed parts,
   // never a reason to loosen the common permissions or fabricate a child.
   const failure=origin==='oc'?'error: denied subagent':'Unknown agent: vis39_missing_agent';
   await shot('subagent-error-collapsed',f=>f.text.includes('Reject missing child profile')&&f.text.includes('VIS39-MISSING-AGENT-DONE')&&!f.text.includes(failure));
  const error=await frame(),ey=error.cells.findIndex(row=>glyphs(row).includes('Reject missing child profile'));
  if(ey<0)throw Error('Actual failed Subagent header missing');
  send(`\x1b[<0;7;${ey+1}M\x1b[<0;7;${ey+1}m`,'subagent_card_expand_actual_error');
   await shot('subagent-error-expanded',f=>f.text.includes('Reject missing child profile')&&f.text.includes(failure));
  const expanded=await frame(),cy=expanded.cells.findIndex(row=>glyphs(row).includes('Reject missing child profile'));
  send(`\x1b[<0;7;${cy+1}M\x1b[<0;7;${cy+1}m`,'subagent_card_collapse_actual_error');
   await shot('subagent-error-recollapsed',f=>f.text.includes('Reject missing child profile')&&!f.text.includes(failure));
  const final=await snapshot('subagent_card_final');
  if(final.composer.effects.started.lines.length!==2||final.composer.effects.completed.lines.length!==1||final.composer.effects.terminal.lines.length!==0||logs.some(e=>e.kind==='provider'&&!e.valid)||logs.filter(e=>e.kind==='fixture_tool_call').length!==5)throw Error('Subagent proof replayed/launched an unexpected effect');
   if(origin==='oc') {
     const rejected=rows(final,'tool_operations').filter(o=>o.name==='subagent'&&JSON.parse(o.input).agent==='vis39_missing_agent');
     if(rejected.length!==1||rejected[0].state!=='failed'||rejected[0].output!==failure)throw Error('Missing profile did not remain the actual pre-effect failed operation');
    const a=rows(settled,'child_jobs'),b=rows(running,'child_jobs'),end=rows(final,'child_jobs');
    if(a.length!==1||b.length!==2||end.length!==2||end.some(j=>j.child_id!==a[0].child_id)||end[0].identity!==a[0].identity||end[0].operation_id!==a[0].operation_id||end.some(j=>j.state!=='completed')||rows(final,'sessions').length!==2||rows(final,'terminals').length!==0)throw Error('Continuation/error changed the original child family or launched a new owner');
    const oldOp=rows(settled,'tool_operations').find(o=>o.id===a[0].operation_id),newOp=rows(final,'tool_operations').find(o=>o.id===a[0].operation_id);
    if(JSON.stringify(oldOp)!==JSON.stringify(newOp))throw Error('New continuation/error rewrote original tool input/output/state');
    const shells=rows(final,'shell_jobs').map(j=>({...j,outcome:JSON.parse(j.outcome)}));
    if(shells.length!==2||shells[0].outcome.state!=='completed'||shells[0].outcome.exit!==0||!shells[0].outcome.display_recent.includes('VIS39-FINAL-FLUSH-first')||shells[1].outcome.state!=='cancelled')throw Error('Original child process final flush/cancellation not frozen');
  }
  return {status:'OBSERVED_SUBAGENT_CARDS',started,converted,settled,running,final,hover_facts:hoverFacts,
    actual_continuation_child:capture.captured_child,geometries:[[120,40],[80,24],[160,48]],
    actual_tool_calls:5,actual_requests:logs.filter(e=>e.kind==='provider').length,
    same_child_distinct_operation:true,missing_agent_no_owner:true,original_graph_unchanged:true,
    shell_starts:2,shell_completions:1,terminal_effects:0,no_effect_replay:true};
}

async function probeRootHandoff({origin,send,shot,waitFor,frame,logs,snapshot,control,resize}) {
  const rows=(s,table)=>s.composer.observations.flatMap(o=>o.data[table]||[]);
  const observe=async(label,predicate)=>{
    const deadline=Date.now()+15000;
    while(Date.now()<deadline) {
      const s=await snapshot(label+'-'+Date.now());
      if(predicate(s.composer))return s;
      await new Promise(resolve=>setTimeout(resolve,50));
    }
    throw Error('Missing Root handoff owner facts '+label);
  };
  const release=async phase=>{
    const request_id='root-release-'+phase;
    control({kind:'tool_preview_control',request_id,action:'release-'+phase});
    const deadline=Date.now()+6000;
    while(!logs.some(e=>e.kind==='tool_preview_control_ack'&&e.request_id===request_id)&&Date.now()<deadline)
      await new Promise(resolve=>setTimeout(resolve,25));
    if(!logs.some(e=>e.kind==='tool_preview_control_ack'&&e.request_id===request_id))throw Error('Missing Root release ACK');
  };
  const hint=f=>f.text.includes('Press ctrl+y to move running work to the background');
  const checkHint=f=>{
    const y=f.cells.findIndex(row=>glyphs(row).includes('Press ctrl+y to move running work to the background'));
    const x=y<0?-1:glyphs(f.cells[y]).indexOf('ctrl+y');
    if(x<0||f.cells[y][x].fg!=='#eeeeee'||f.cells[y][x-1].fg!=='#808080')throw Error('Hint key/sentence did not retain base/muted semantic styles');
  };
  const sameProcess=(before,after)=>before.operation_id===after.operation_id&&before.session_id===after.session_id&&before.process===after.process&&before.provenance===after.provenance;
  await shot('root-handoff-home',f=>f.text.includes('Ask anything'));
  send('\x1b[200~VIS39_ROOT_HANDOFF_TASK\x1b[201~\r','root_handoff_submit');
  const initial=await observe('root-child-foreground',s=>s.ready.first&&s.effects.started.lines.length===1);
  for(const [columns,height] of [[120,40],[80,24],[160,48]]) {
    await resize(height,columns);
    const f=await shot('root-child-hint-'+columns,f=>f.columns===columns&&f.rows===height&&hint(f)&&f.text.includes('Inspect child shell'));
    checkHint(f);
  }
  const originalChild=origin==='oc'?rows(initial,'child_jobs')[0]:null;
  const originalProcess=origin==='oc'?rows(initial,'shell_jobs')[0]:null;
  if(originalChild&&(originalChild.converted_background!==0||originalProcess.session_id!==originalChild.child_id||originalProcess.converted_background!==0||originalProcess.phase!=='running'))throw Error('First foreground source was not the actual child blocker');
  await resize(40,120);
  send('\x19','root_handoff_all_current_child');
  const second=await observe('root-own-shell-foreground',s=>s.ready.second&&s.effects.started.lines.length===2&&s.effects.completed.lines.length===0);
  let rootProcess;
  if(origin==='oc') {
    const currentChild=rows(second,'child_jobs')[0], shells=rows(second,'shell_jobs');
    rootProcess=shells.find(job=>job.session_id===currentChild.parent_id);
    const childProcess=shells.find(job=>job.session_id===currentChild.child_id);
    if(!rootProcess||!sameProcess(originalProcess,childProcess)||currentChild.identity!==originalChild.identity||currentChild.operation_id!==originalChild.operation_id||currentChild.converted_background!==1||childProcess.converted_background!==0||rootProcess.foreground_admitted!==1||rootProcess.converted_background!==0)throw Error('Root child handoff retargeted a nested Shell or replaced its original family/process');
  }
  await shot('root-child-background',f=>f.text.includes('Background')&&f.text.includes('composer-probe.py second'));
  const shellHint=await shot('root-shell-hint',f=>hint(f)&&f.text.includes('composer-probe.py second')&&f.text.includes('VIS39-ROW-079'));
  checkHint(shellHint);
  send('\x19','root_handoff_all_current_shell');
  await waitFor(f=>f.text.includes('VIS39-ROOT-HANDOFF-DONE')&&!hint(f),'Root continued after actual Shell handoff',15000);
  const converted=await snapshot('root-handoff-converted');
  if(origin==='oc') {
    const current=rows(converted,'shell_jobs').find(job=>job.operation_id===rootProcess.operation_id);
    if(!sameProcess(rootProcess,current)||current.converted_background!==1||current.phase!=='running')throw Error('Root Shell handoff did not retain the same admitted foreground process');
  }
  send('\x1b[200~Root handoff unsent Ω界\x1b[201~','root_handoff_preserved_draft');
  for(const [columns,height] of [[80,24],[120,40],[160,48]]) {
    await resize(height,columns);
    await shot('root-background-footer-'+columns,f=>f.columns===columns&&f.rows===height&&f.text.includes('Root handoff unsent Ω界')&&f.text.includes('VIS39-ROOT-HANDOFF-DONE')&&!hint(f));
  }
  await release('first');await release('second');
  const completed=await observe('root-handoff-frozen',s=>s.effects.completed.lines.length===2);
  const settled=await observe('root-handoff-child-settled',s=>origin!=='oc'||s.observations.flatMap(o=>o.data.child_jobs||[]).every(job=>job.state==='completed'));
  await resize(40,120);
  const final=await shot('root-handoff-final',f=>f.text.includes('Root handoff unsent Ω界')&&f.text.includes('VIS39-ROOT-HANDOFF-DONE')&&!hint(f));
  const counts=s=>s.composer.effects;
  if(counts(completed).started.lines.join()!=='first,second'||new Set(counts(completed).completed.lines).size!==2||counts(completed).terminal.lines.length)throw Error('Root handoff replayed or lost real effects');
  if(origin==='oc') {
    const shells=rows(settled,'shell_jobs');
    if(shells.length!==2||shells.some(job=>job.phase!=='terminal'||JSON.parse(job.outcome).state!=='completed'||JSON.parse(job.outcome).exit!==0||!JSON.parse(job.outcome).display_recent.includes('VIS39-FINAL-FLUSH-'))||rows(settled,'terminals').length)throw Error('Root handoff lacked actual final frozen flushes/no-PTY facts');
    const label=final.cells.find(row=>glyphs(row).includes('Build · Fixture Caption Model'));
    if(!label||label[glyphs(label).indexOf('Build')].fg!=='#12ab34')throw Error('Root footer ignored the explicit admitted profile color: '+(label?.[glyphs(label).indexOf('Build')]?.fg??'missing footer'));
  }
  const calls=logs.filter(e=>e.kind==='fixture_tool_call');
  if(calls.length!==3||calls.filter(e=>e.name==='subagent').length!==1||calls.filter(e=>e.name==='shell').length!==2||logs.some(e=>e.kind==='provider'&&!e.valid))throw Error('Root handoff changed actual structured calls');
  return {status:'PASS_BEHAVIOR_ONLY',initial,second,converted,settled,actual_calls:calls.length,
    actual_requests:logs.filter(e=>e.kind==='provider').length,root_prompt_remapped_key:'ctrl+y',
    preserved_source_and_process:true,nested_child_shell_not_root_target:true,no_effect_replay:true,
    scopes:'Both sides use their actual focused Root remapped background binding; no composer/viewer shortcut or API view injection. Native delay uses actual live receipt time, original its stored part start.'};
}

async function probeRootModelShell({origin,send,shot,waitFor,frame,logs,snapshot,control}) {
  const rows=(s,table)=>s.composer.observations.flatMap(o=>o.data[table]||[]);
  const observe=async(label,predicate)=>{
    const deadline=Date.now()+15000;
    while(Date.now()<deadline){const s=await snapshot(label+'-'+Date.now());if(predicate(s.composer))return s;await new Promise(r=>setTimeout(r,50));}
    throw Error('Timed out actual root model Shell '+label);
  };
  const openShells=async()=>{
    if(origin==='oc')send('\x07','root_model_open_composer');
    else {
      const f=await frame(),glyphs=row=>row.map(c=>c.width===0?'':c.symbol).join('');
      const y=f.cells.findIndex(row=>glyphs(row).includes('ctrl+g 1 shell')||glyphs(row).includes('ctrl+g 2 shells'));
      const x=y<0?-1:glyphs(f.cells[y]).indexOf('ctrl+g');
      if(x<0)throw Error('Actual root live Shell status action is absent');
      send(`\x1b[<0;${x+1};${y+1}M\x1b[<0;${x+1};${y+1}m`,'root_model_live_shell_status_click');
    }
    await waitFor(f=>f.text.includes('Subagents')&&f.text.includes('Terminals'),'actual root composer',6000);
    send('\x1b[C','root_model_shell_tab');
    await waitFor(f=>activeTab(f,'Shell')&&f.text.includes('python3 composer-probe.py first'),'actual running source Shell',6000);
  };
  const release=async()=>{
    control({kind:'tool_preview_control',request_id:'release-first',action:'release-first'});
    await observe('first_released',s=>s.effects.completed.lines.length===1);
  };
  await shot('model-shell-home',f=>f.text.includes('Ask anything'));
  send('\x1b[200~VIS39_ROOT_MODEL_SHELL_TASK\x1b[201~\r','root_model_submit');
  const started=await observe('first_ready',s=>s.ready.first&&s.effects.started.lines.length===1);
  await shot('model-shell-foreground',f=>f.text.includes('VIS39-ROW-079')&&f.text.includes('VIS39-STDERR-first'));
  // The original action belongs to its focused root prompt. Native exposes the
  // existing exact-job converter in its captured output viewer. Exercise each
  // real scope, not an API call masquerading as a key or an identical-key claim.
  if(origin==='oc'){
    await openShells();send('\r','root_model_original_output');
    await waitFor(f=>f.text.includes('Shell output')&&f.text.includes('Running'),'captured model output',6000);
    send('\x02','root_model_background_captured_job');
    await observe('second_ready',s=>s.ready.second&&s.effects.started.lines.length===2);
    send('\x1b','root_model_close_viewer');
    await waitFor(f=>!f.text.includes('Shell output'),'closed actual captured viewer',6000);
    send('\x1b','root_model_close_composer');
    await waitFor(f=>!f.text.includes('Subagents  Shell  Terminals'),'closed actual root composer',6000);
  }else{
    send('\x02','root_model_background_base_session');
    await observe('second_ready',s=>s.ready.second&&s.effects.started.lines.length===2);
  }
  await shot('model-shell-background',f=>f.text.includes('Background')&&f.text.includes('composer-probe.py second'));
  // Expanding the real eighty-line capture anchors native at its header, while
  // the original may follow its tail. Navigate the actual viewport on each
  // side; neither an offscreen header nor offscreen stderr is a missing card.
  const scrollTo=async(label,predicate,up)=>{
    // The fixture publishes eighty rows plus two cards' chrome. At 80x24 a
    // wheel may paint before its kinetic movement has fully settled, so twenty
    // four acknowledged progress steps do not necessarily cross that content.
    // Keep navigation finite, and still require actual movement for each step.
    for(let n=0;n<128;n++){
      const f=await frame();if(predicate(f))return;
      // Pinned session.page.up/down owns the original transcript; native's
      // current transcript navigation is its actual wheel owner.
      // Three real wheel ticks are a short user burst, not an offset injection.
      // Crossing eighty rows one IPC/frame roundtrip per tick can exhaust the
      // unchanged 120-second process guard at the largest styled-grid size.
      send(origin==='upstream'?(up?'\x1b[5~':'\x1b[6~'):`\x1b[<${up?64:65};10;10M`.repeat(3),label+'-'+n);
      // This is an input-progress observation, not a still-shot qualification:
      // unrelated live styles need not settle before sending the next scroll.
      const end=Date.now()+2000;let moved=false;
      while(Date.now()<end){
        const next=await frame();if(next.text!==f.text){moved=true;break;}
        await new Promise(r=>setTimeout(r,50));
      }
      if(!moved)throw Error('Actual viewport did not move for '+label);
    }
    throw Error('Actual model Shell viewport did not reach '+label);
  };
  const toggleFirstCard=async(label)=>{
    await scrollTo(label+' header',f=>f.text.includes('python3 composer-probe.py first'),true);
    const f=await frame(),glyphs=row=>row.map(c=>c.width===0?'':c.symbol).join('');
    const y=f.cells.findIndex(row=>glyphs(row).includes('python3 composer-probe.py first'));
    if(y<0)throw Error('First model Shell card is not painted');
    send(`\x1b[<0;6;${y+1}M\x1b[<0;6;${y+1}m`,label);
  };
  await toggleFirstCard('root_model_expand_existing_card');
  await waitFor(f=>f.text.includes('VIS39-LIVE-first')||f.text.includes('VIS39-STDERR-first'),'actual expanded model Shell body',6000);
  await scrollTo('expanded model Shell stderr',f=>f.text.includes('Background')&&f.text.includes('VIS39-STDERR-first'),false);
  await shot('model-shell-background-expanded',f=>f.text.includes('Background')&&f.text.includes('VIS39-STDERR-first'));
  await toggleFirstCard('root_model_collapse_existing_card');
  await shot('model-shell-background-collapsed',f=>f.text.includes('Background')&&!f.text.includes('VIS39-STDERR-first'));
  const converted=await snapshot('model_shell_converted');
  if(converted.composer.effects.completed.lines.length!==0)throw Error('Conversion executed or completed the held job');
  let ownerFacts=null;
  if(origin==='oc'){
    const sessions=rows(converted,'sessions'),firstBefore=rows(started,'shell_jobs')[0];
    const first=rows(converted,'shell_jobs').find(j=>j.operation_id===firstBefore?.operation_id);
    if(sessions.length!==1||sessions[0].parent_id!==null||rows(converted,'child_jobs').length!==0||!first||first.process!==firstBefore.process||firstBefore.foreground_admitted!==1||first.converted_background!==1)throw Error('Root model background conversion lost its original process/source');
    const p=JSON.parse(first.provenance);if(p.session!==sessions[0].id||p.operation!==first.operation_id||!p.turn)throw Error('Model job provenance does not match the root tool graph');
    ownerFacts={source:sessions[0].id,operation:first.operation_id,same_process_conversion:true};
  }
  await openShells();send('\r','root_model_reopen_original_capture');
  await shot('model-shell-background-output',f=>f.text.includes('Shell output')&&f.text.includes('Running')&&f.text.includes('VIS39-ROW-079'));
  await release();
  await shot('model-shell-final-output',f=>f.text.includes('Shell output')&&f.text.includes('Exited')&&f.text.includes('VIS39-FINAL-FLUSH-first'));
  send('\x1b','root_model_close_final_output');
  await shot('model-shell-second',f=>f.text.includes('composer-probe.py second')&&!f.text.includes('Shell output'));
  send('\x04','root_model_kill_selected_second');
  await waitFor(f=>f.text.includes('VIS39-ROOT-SHELL-DONE'),'actual root tool graph settled',15000);
  send('\x1b','root_model_close_composer');
  const final=await snapshot('model_shell_final');
  if(final.composer.effects.started.lines.length!==2||final.composer.effects.completed.lines.length!==1||logs.filter(e=>e.kind==='fixture_tool_call').length!==2||logs.some(e=>e.kind==='provider'&&!e.valid))throw Error('Root model Shell effects or structured graph replayed');
  if(ownerFacts){
    const jobs=rows(final,'shell_jobs'),first=jobs.find(j=>j.operation_id===ownerFacts.operation);
    const outcome=JSON.parse(first.outcome),facts=outcome.output_presentation?.shell;
    if(!facts?.background||facts.process_state!=='completed'||facts.exit!==0||!outcome.display_recent?.includes('VIS39-FINAL-FLUSH-first')||jobs.length!==2||!jobs.some(j=>JSON.parse(j.outcome).state==='cancelled'))throw Error('Frozen model facts do not retain the actual conversion/final flush/cancellation');
    ownerFacts.frozen_background_final_flush=true;
  }
  return {status:'OBSERVED_COMPOSER',origin,started,converted,final,owner_facts:ownerFacts,
    actual_tool_calls:2,actual_requests:logs.filter(e=>e.kind==='provider').length,
    shell_starts:2,shell_completions:1,owner_snapshot_read_only:true,no_effect_replay:true};
}
