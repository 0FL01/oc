// Actual ! composer and session.shell / supervised Jobs, never model tool calls.
const COMMAND='python3 user-shell-probe.py';
const glyphs=row=>(row||[]).filter(c=>c.width!==0).map(c=>c.symbol).join('');
const promptHas=(f,text)=>f.cursor.visible&&glyphs(f.cells[f.cursor.y]).includes(text);
const shellCaption=f=>f.cursor.visible&&glyphs(f.cells[f.cursor.y+2]).trim().replace(/^[┃│]\s*/,'')==='Shell';
export async function probeUserShell({origin,send,shot,waitFor,frame,logs,snapshot,relaunch}) {
  const requests=()=>logs.filter(e=>e.kind==='provider');
  const verify=(s,count)=>{
    if(requests().length||s.mcp_calls.length||s.shell_effect?.lines!==count||s.user_shell_boundary.length!==count)throw Error('User Shell request/effect boundary disagrees');
    if(origin==='oc') {
      const data=s.observations.map(o=>o.data).find(d=>d.prompt_input_history);
      if(JSON.stringify(data?.prompt_input_history)!==JSON.stringify([COMMAND])||data.model_turns!==0||data.tool_operations.length!==count||data.tool_operations.some(o=>o.turn_id!==null||o.state!=='completed'||JSON.parse(o.input).command!==COMMAND))throw Error('Native User Shell history/NULL-turn completion disagrees');
      if(s.user_shell_boundary.some(f=>!f.history_exact||f.model_turns!==0||f.turn!==null||f.state!=='started'))throw Error('Native history was not committed before effect');
      if(new Set(s.user_shell_boundary.map(f=>f.operation)).size!==count)throw Error('User Shell operation replayed');
    }
  };
  await shot('user-shell-home',f=>f.text.includes('Ask anything'));
  send('!','user_shell_enter_mode');
  await shot('user-shell-mode',f=>f.text.includes('Run a command')&&f.text.includes('Shell'));
  send('\x1b','user_shell_exit_mode');
  await shot('user-shell-escape',f=>f.text.includes('Ask anything'));
  send('!','user_shell_reenter_mode');
  await waitFor(f=>f.text.includes('Run a command'),'reenter Shell',6000);
  send(`\x1b[200~${COMMAND}\x1b[201~`,'user_shell_exact_command');
  await shot('user-shell-input',f=>promptHas(f,COMMAND)&&f.text.includes('Shell'));
  const before=await snapshot('user_shell_before_enter');
  const shellInput=await frame();
  if(!shellInput.text.includes('esc exit shell mode')||shellInput.text.includes('shift+tab agents')||shellInput.text.includes('ctrl+p commands'))throw Error('Shell input retained normal-mode footer hints');
  if(requests().length||before.shell_effect!==null||before.user_shell_boundary.length||before.mcp_calls.length)throw Error('Shell mode/edit launched an effect');
  send('\r','user_shell_explicit_submit');
  const running=await shot('user-shell-running',f=>f.text.includes(COMMAND)&&f.text.includes('↓ 1 shell')&&!f.text.includes('VIS-USER-SHELL-DONE')&&!f.text.includes('admission pending')&&!promptHas(f,COMMAND));
  if(running.text.includes('native admission; data only'))throw Error('Running User Shell is still an ordinary user notice');
  const during=await snapshot('user_shell_running_before_effect');
  if(requests().length||during.shell_effect!==null||during.mcp_calls.length)throw Error('Held running Shell had an early effect');
  if(origin==='oc') {
    const data=during.observations.map(o=>o.data).find(d=>d.prompt_input_history);
    if(data?.model_turns!==0||data?.tool_operations.length!==1||data?.tool_operations[0].turn_id!==null||data?.tool_operations[0].state!=='started')throw Error('Running Shell does not have its genuine NULL-turn intent');
  }
   const completed=await shot('user-shell-completed',f=>f.text.includes('VIS-USER-SHELL-DONE')&&!f.text.includes('interrupt'));
   if(completed.text.includes('↓ 1 shell'))throw Error('Completed Shell retained a stale live inventory indicator');
  const structured=(f,count)=>{
    const rows=f.cells.map(glyphs);
    if(!rows[0].includes('New session')||rows[0].includes('Untitled session'))throw Error('Fresh Shell lost its tab-only New session fallback');
    if(rows.filter(row=>row.includes(`$ ${COMMAND}`)).length!==count||f.text.includes('native admission; data only')||f.text.includes('native durable notice')||f.text.includes('Command exited with code 0'))throw Error('User Shell completion is not a single structured command/output block');
  };
  structured(completed,1);
  const first=await snapshot('user_shell_first_completed');verify(first,1);
  send('\x1b[A','user_shell_recall_only');
  const recall=await shot('user-shell-recalled',f=>promptHas(f,COMMAND));
  const recalled=await snapshot('user_shell_recalled');verify(recalled,1);
  const recalledShellMode=shellCaption(recall);
  // The original restores its recorded Shell mode; native's already-qualified
  // shared text history does not claim parts/mode/Mini restoration. Observe
  // the actual difference, then explicitly exit/reenter before another run.
  if(recalledShellMode)send('\x1b','user_shell_exit_recalled_mode');
  await shot('user-shell-recalled-normal',f=>promptHas(f,COMMAND)&&!shellCaption(f));
  send('!','user_shell_recalled_mode');
  await shot('user-shell-recalled-mode',f=>promptHas(f,COMMAND)&&f.text.includes('Shell'));
  send('\r','user_shell_second_explicit_submit');
  // Observe durable completion independently of an unchanged repeated stdout.
  let second;
  for(let n=0;n<80;n++) {
    second=await snapshot('user_shell_second_'+n);
    if(second.shell_effect?.lines===2&&second.observations.every(o=>!(o.data.tool_operations||[]).some(op=>op.state==='started')))break;
    await new Promise(r=>setTimeout(r,25));
  }
  verify(second,2);
  const secondCompleted=await shot('user-shell-second-completed',f=>f.text.includes('VIS-USER-SHELL-DONE')&&!f.text.includes('interrupt'));
  structured(secondCompleted,2);
  send('\x03','user_shell_clean_exit');
  const deadline=Date.now()+6000;while(!logs.some(e=>e.kind==='exit'&&e.generation===0)&&Date.now()<deadline)await new Promise(r=>setTimeout(r,25));
  if(!logs.some(e=>e.kind==='exit'&&e.generation===0))throw Error('User Shell process did not exit');
  await relaunch();
  const restartedHome=await shot('user-shell-restarted-home',f=>f.text.includes('Ask anything')||f.text.includes('VIS-USER-SHELL-DONE'));
  if(glyphs(restartedHome.cells[0]).includes('Untitled session'))throw Error('Restart lost the persisted tab fallback');
  send('\x1b[A','user_shell_restarted_recall');
  await shot('user-shell-restarted-recalled',f=>promptHas(f,COMMAND));
  const restarted=await snapshot('user_shell_restarted');verify(restarted,2);
  return {status:'OBSERVED_USER_SHELL',command:COMMAND,before,during,first,recalled,second,restarted,
    provider_requests:0,explicit_commands:2,effects:2,recalled_shell_mode:recalledShellMode,
    native_history_before_effect:origin==='oc',new_session_tab_fallback:true,no_restart_replay:true};
}
