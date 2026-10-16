// Real PTY input and read-only snapshots; no source-rendered frames or fake effects.
export async function probeExecute({origin,send,shot,waitFor,frame,visibleMatches,logs,snapshot,relaunch,resize,slash,control,readVt}) {
  const requests=()=>logs.filter(event=>event.kind==='provider').length;
  const calls=()=>logs.filter(event=>event.kind==='fixture_tool_call').length;
  const clickPoint=point=>send(`\x1b[<0;${point.x+1};${point.y+1}M\x1b[<0;${point.x+1};${point.y+1}m`,'execute_click');
  const parent=f=>visibleMatches(f,'execute').filter(point=>f.cells[point.y].map(cell=>cell.symbol).join('').trimEnd().endsWith(' execute'));
  const open=async()=>{
    const hits=parent(await frame());
    if(hits.length!==1)throw Error('Execute parent is not unique');
    clickPoint(hits[0]);
  };
  const release=action=>control({kind:'tool_preview_control',request_id:action,action});
  const code=()=>logs.find(event=>event.kind==='fixture_tool_call'&&event.index===1)?.arguments.code;
  const clipboard=async(key,expected)=>{
    const offset=readVt().length;
    send(key,'execute_copy_'+key);
    const copied=()=>[...readVt().slice(offset).matchAll(/\x1b\]52;[^;]*;([A-Za-z0-9+/=]*)(?:\x07|\x1b\\)/g)]
      .map(match=>Buffer.from(match[1],'base64').toString('utf8'));
    await waitFor(()=>copied().length>0,'actual OSC52 '+key,6000);
    if(copied().at(-1)!==expected)throw Error('Execute copied bytes differ from available captured '+key);
    await shot('execute-copied-'+key,f=>f.text.includes('copied'));
  };
  const unchanged=(before,after,expectedRequests,expectedCalls)=>{
    for(const key of ['artifacts','mcp_calls','observations','shell_effect'])
      if(JSON.stringify(before[key])!==JSON.stringify(after[key]))throw Error('Execute replay changed '+key);
    if(requests()!==expectedRequests||calls()!==expectedCalls)throw Error('Execute view replayed a model/tool call');
  };
  const exit=async generation=>{
    send('\x03','execute_exit');
    const end=Date.now()+6000;
    while(!logs.some(event=>event.kind==='exit'&&event.generation===generation)) {
      if(Date.now()>end)throw Error('Execute fixture did not exit');
      await new Promise(resolve=>setTimeout(resolve,25));
    }
    const event=logs.find(event=>event.kind==='exit'&&event.generation===generation);
    if(event.code!==0||event.termination!=='natural')throw Error('Execute fixture did not restore naturally');
    return {generation,code:event.code,termination:event.termination};
  };
  await waitFor(f=>f.text.includes('Ask anything')&&f.text.includes('1 MCP /mcps'),'execute ready',15000);
  send('\x1b[200~VIS46 execute the supplied program exactly once.\x1b[201~','execute_prompt');
  send('\r','execute_submit');
  await shot('execute-receiving',f=>parent(f).length===1&&logs.some(event=>event.kind==='execute_receiving_held'));
  await open();
  await shot('execute-receiving-dialog',f=>f.text.includes('Receiving code')&&f.text.includes('Code'));
  release('release-code');
  await shot('execute-running-dialog',f=>f.text.includes('Running')&&f.text.includes('const first')&&logs.some(event=>event.kind==='execute_fetch'));
  await clipboard('c',code());
  const running=await snapshot('execute-running');
  if(running.mcp_calls.length!==2||logs.filter(event=>event.kind==='execute_fetch').length!==1)
    throw Error('Execute did not perform its two real MCP calls and one held fetch');
  release('release-fetch');
  await shot('execute-completed-dialog',f=>f.text.includes('Completed')&&f.text.includes('VIS46-RESULT')&&f.text.includes('Output'));
  const result=logs.find(event=>event.kind==='execute_expected_output');
  if(!result)throw Error('Missing structured fixture output contract');
  await clipboard('o',result.text);
  const unpanned=await frame();
  send('\x1b[C','execute_pan');
  await shot('execute-panned-dialog',f=>JSON.stringify(f.cells)!==JSON.stringify(unpanned.cells));
  send('\x1b[F','execute_scroll_end');
  await shot('execute-output-end',f=>f.text.includes('VIS46 actual log'));
  for(const [columns,rows] of [[80,24],[160,48],[120,40]]) {
    await resize(rows,columns);
    await shot('execute-dialog-'+columns+'x'+rows,f=>f.columns===columns&&f.rows===rows&&f.text.includes('Completed'));
  }
  send('\x1b','execute_close');
  await shot('execute-completed',f=>f.text.includes('VIS46-DONE')&&f.cursor.visible&&
    !f.text.split('\n').some(line=>line.trim()==='Code'));
  send('\x1b[200~preserved draft\x1b[201~','execute_draft');
  const draft=await waitFor(f=>f.text.includes('preserved draft'),'execute draft',6000);
  const child=visibleMatches(draft,'visexecute.output [index=2');
  if(child.length!==1)throw Error('Execute child is not unique');
  send(`\x1b[<35;${child[0].x+1};${child[0].y+1}M`,'execute_hover');
  await shot('execute-child-hover',f=>JSON.stringify(f.cells)!==JSON.stringify(draft.cells));
  clickPoint(child[0]);
  const expanded=await shot('execute-child-expanded',f=>f.text.includes('nested:')&&f.text.includes('first'));
  if(JSON.stringify(expanded.cursor)!==JSON.stringify(draft.cursor)||!expanded.text.includes('preserved draft'))
    throw Error('Execute child stole composer draft/caret');
  const again=visibleMatches(expanded,'visexecute.output').filter(p=>
    expanded.cells[p.y].map(cell=>cell.symbol).join('').trim()==='› visexecute.output');
  if(again.length!==1)throw Error('Expanded execute child is not unique');
  clickPoint(again[0]);
  await shot('execute-child-recollapsed',f=>!f.text.includes('nested:'));
  const completed=await snapshot('execute-completed');
  if(requests()!==3||calls()!==1||completed.mcp_calls.length!==2)throw Error('Execute success request graph differs');
  send('\x03','execute_clear_draft');
  await waitFor(f=>!f.text.includes('preserved draft'),'execute draft cleared',6000);
  send('\x1b[200~VIS46 exercise the supplied failed program exactly once.\x1b[201~','execute_error_prompt');
  send('\r','execute_error_submit');
  await shot('execute-error',f=>f.text.includes('VIS46-ERROR-DONE')&&f.text.includes('VIS46 ERROR'));
  const failedFrame=await frame();
  const failedParents=parent(failedFrame).filter(point=>
    failedFrame.cells[point.y].map(cell=>cell.symbol).join('').trim()==='✗ execute');
  if(failedParents.length!==1)throw Error('Failed execute parent is not unique');
  clickPoint(failedParents[0]);
  // The pinned producer returns interpreter errors as completed ToolState plus
  // metadata.error (core/codemode/tool.ts:114–137). Its InlineTool therefore opens
  // Failed detail. The frozen native contract instead owns a failed parent and
  // requires inline expansion. Observe both actual actions without equating the
  // states, changing native policy, or normalizing their full-frame differences.
  if(origin==='oc') {
    await shot('execute-error-expanded',f=>f.text.includes('line f')&&f.cursor.visible);
    const expandedError=await frame();
    const closeError=parent(expandedError).filter(point=>
      expandedError.cells[point.y].map(cell=>cell.symbol).join('').trim()==='✗ execute');
    if(closeError.length!==1)throw Error('Expanded failed execute parent is not unique');
    clickPoint(closeError[0]);
  } else {
    await shot('execute-error-expanded',f=>f.text.includes('line f')&&f.text.includes('Failed')&&!f.cursor.visible);
    send('\x1b','execute_metadata_error_dialog_close');
  }
  await shot('execute-error-recollapsed',f=>!f.text.includes('line f')&&f.text.includes('line d…')&&f.cursor.visible);
  const failed=await snapshot('execute-error');
  if(requests()!==5||calls()!==2||failed.mcp_calls.length!==3||logs.some(event=>event.kind==='provider'&&!event.valid))
    throw Error('Execute error request graph differs');
  if(origin==='oc') {
    const operations=failed.observations.flatMap(item=>item.data.tool_operations||[]);
    const records=failed.observations.flatMap(item=>item.data.presentation||[]).filter(record=>!record.presentation.label_only);
    if(operations.length!==2||operations.some(op=>op.name!=='execute')||records.length!==2)
      throw Error('Execute created nested durable operations or lost final facts');
    const success=records[0].presentation.execute,error=records[1].presentation.execute;
    if(!success||success.error||success.calls.length!==3||success.calls.some(call=>call.status!=='completed')||
       !error?.error||error.calls.length!==1||error.calls[0].status!=='error')throw Error('Execute final typed facts differ');
  }
  await slash('/new');
  await waitFor(f=>f.text.includes('Ask anything'),'execute new home',6000);
  const reopen=async stage=>{
    await slash('/sessions');
    const listing=await waitFor(f=>f.text.includes('Sessions')&&f.text.includes('VIS46 execute fixture'),'execute session list',6000);
    const hits=visibleMatches(listing,'VIS46 execute fixture').filter(point=>point.y>0);
    if(hits.length!==1)throw Error('Execute session title is not unique');
    clickPoint(hits[0]);
    await shot(stage,f=>f.text.includes('VIS46-ERROR-DONE')&&f.text.includes('VIS46 ERROR'));
  };
  await reopen('execute-reopened');
  unchanged(failed,await snapshot('execute-reopened'),5,2);
  const exits=[await exit(0)];
  await relaunch();
  await waitFor(f=>f.text.includes('Ask anything')||f.text.includes('VIS46-ERROR-DONE'),'execute restarted',15000);
  await reopen('execute-restarted');
  const after=await snapshot('execute-restarted');
  unchanged(failed,after,5,2);
  exits.push(await exit(1));
  return {before:failed,after,exits,receiving_follows_running_and_final:true,
    actual_clipboard_code_output:true,exact_request_graph:true,no_tool_replay:true,
    error_interaction:origin==='oc'?'failed_parent_inline_toggle':'completed_metadata_error_failed_dialog',
    error_state_equivalence:false};
}
