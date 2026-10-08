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
  if(requests().length||before.shell_effect!==null||before.user_shell_boundary.length||before.mcp_calls.length)throw Error('Shell mode/edit launched an effect');
  send('\r','user_shell_explicit_submit');
  await shot('user-shell-completed',f=>f.text.includes('VIS-USER-SHELL-DONE')&&!f.text.includes('interrupt'));
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
  await shot('user-shell-second-completed',f=>f.text.includes('VIS-USER-SHELL-DONE')&&!f.text.includes('interrupt'));
  send('\x03','user_shell_clean_exit');
  const deadline=Date.now()+6000;while(!logs.some(e=>e.kind==='exit'&&e.generation===0)&&Date.now()<deadline)await new Promise(r=>setTimeout(r,25));
  if(!logs.some(e=>e.kind==='exit'&&e.generation===0))throw Error('User Shell process did not exit');
  await relaunch();
  await shot('user-shell-restarted-home',f=>f.text.includes('Ask anything')||f.text.includes('VIS-USER-SHELL-DONE'));
  send('\x1b[A','user_shell_restarted_recall');
  await shot('user-shell-restarted-recalled',f=>promptHas(f,COMMAND));
  const restarted=await snapshot('user_shell_restarted');verify(restarted,2);
  return {status:'OBSERVED_USER_SHELL',command:COMMAND,before,first,recalled,second,restarted,
    provider_requests:0,explicit_commands:2,effects:2,recalled_shell_mode:recalledShellMode,
    native_history_before_effect:origin==='oc',no_restart_replay:true};
}
