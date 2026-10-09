// Real parent delegation, child-owned Shell and terminal effects, no UI stories.
export async function probeComposer({origin,send,shot,waitFor,frame,logs,snapshot,control}) {
  const glyphs=row=>row.map(c=>c.width===0?'':c.symbol).join('');
  const activeTab=(f,label)=>f.cells.some(row=>{
    const text=glyphs(row),x=text.indexOf(label);
    return text.includes('Subagents')&&text.includes('Terminals')&&x>=0&&row[x]?.modifiers.includes('bold');
  });
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
  if(origin==='oc'&&!(await frame()).text.split('\n')[0].includes('VIS39 parent'))throw Error('Linked child replaced the retained root-deck title');
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
  await shot('composer-parent-restored',f=>f.text.includes('VIS39 parent unsent draft'));
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
      second_shell_cancelled:true,undefined_terminal_enter_noop:true,same_terminal_hide_show:true};
  }
  return {status:'OBSERVED_COMPOSER',started,converted,second,undefinedEnter,terminal,final,owner_facts:ownerFacts,
    origin,actual_tool_calls:3,actual_requests:requestCount(),shell_starts:2,shell_completions:1,
    terminal_effects:1,owner_snapshot_read_only:true,no_effect_replay:true};
}
