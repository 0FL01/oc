// Comparable bounded outputs, real timing and effects through unchanged binaries.
export async function probeSmallToolPreview({origin,send,shot,waitFor,frame,visibleMatches,logs,snapshot,relaunch,resize,slash,reopen,click,target}) {
  const requests=()=>logs.filter(event=>event.kind==='provider').length;
  const calls=()=>logs.filter(event=>event.kind==='fixture_tool_call').length;
  const unchanged=(before,after)=>{
    for(const key of ['artifacts','mcp_calls','observations','shell_effect'])
      if(JSON.stringify(before[key])!==JSON.stringify(after[key]))throw Error('Small preview changed '+key);
    if(requests()!==5||calls()!==3)throw Error('Small preview replayed provider/tool effects');
  };
  const naturalExit=async generation=>{
    send('\x03','tool_preview_exit');
    const end=Date.now()+6000;
    while(!logs.some(event=>event.kind==='exit'&&event.generation===generation)) {
      if(Date.now()>end)throw Error('Small preview did not exit naturally');
      await new Promise(resolve=>setTimeout(resolve,25));
    }
    const event=logs.find(event=>event.kind==='exit'&&event.generation===generation);
    if(event.code!==0||event.termination!=='natural')throw Error('Small preview exit was not clean');
    return {generation,code:event.code,termination:event.termination};
  };
  await waitFor(f=>f.text.includes('Ask anything')&&f.text.includes('1 MCP /mcps'),'small preview ready',15000);
  send('\x1b[200~VIS16 preview: execute the supplied MCP and Shell calls exactly once.\x1b[201~','tool_preview_prompt');
  send('\r','tool_preview_submit');
  const completed=await shot('small-completed',f=>f.text.includes('VIS16-DONE:')&&target(f,1).length===1);
  if(requests()!==5||calls()!==3||logs.some(event=>event.kind==='provider'&&!event.valid))throw Error('Small preview invalid request graph');
  const timing=logs.filter(event=>event.kind==='fixture_real_elapsed_barrier');
  if(timing.length!==1||timing[0].target_ms!==5500||timing[0].response_ns<timing[0].deadline_ns||
    timing[0].clock_basis!==(origin==='upstream'?'original_user_time_created':'pty_submission'))
    throw Error('Small preview lacks actual bounded response wait');
  const before=await snapshot('small-completed');
  if(before.mcp_calls.length!==2||before.shell_effect?.lines!==1||before.shell_effect?.bytes!==17)
    throw Error('Small preview lacks exact real effects');
  const facts={before,timing:timing[0],actual_footer_5500ms_bucket:completed.text.includes('5.5s')};
  if(origin==='oc') {
    const records=before.observations.flatMap(item=>item.data.presentation||[]);
    const operations=before.observations.flatMap(item=>item.data.tool_operations||[]);
    const results=records.filter(record=>!record.presentation.label_only);
    if(operations.length!==3||results.length!==4||records.length!==6||results.some(record=>
      record.presentation.body_limited||record.presentation.producer_limited||
      Buffer.byteLength(record.presentation.body)>2048||record.presentation.body.includes('[tool output:')))
      throw Error('Small native result is missing, limited or synthetic guidance');
    const mcp=results.filter(record=>record.presentation.mcp_label);
    if(mcp.length!==2||mcp.some(record=>record.presentation.mcp_label.text!=='vispreview_output'))
      throw Error('Small MCP display identity did not follow captured facts');
    facts.bounded_complete_results=true;
  }
  send('\x1b[200~preserved draft\x1b[201~','tool_preview_draft');
  const base=await waitFor(f=>f.text.includes('preserved draft'),'small draft',6000);
  const caret=base.cursor;
  const draftChecks=f=>({draft_preserved:f.text.includes('preserved draft'),prompt_caret_preserved:JSON.stringify(f.cursor)===JSON.stringify(caret)});
  const position=target(base,1)[0];
  send(`\x1b[<35;${position.x+1};${position.y+1}M`,'tool_preview_hover');
  const hovered=await shot('small-hover',f=>JSON.stringify(f.cells)!==JSON.stringify(base.cells)&&!f.text.includes('output: VIS-MCP-FIRST'));
  if(!Object.values(draftChecks(hovered)).every(Boolean))throw Error('Small hover stole draft/caret');
  for(const index of [1,2]) {
    await click(index);
    const label=index===1?'FIRST':'SECOND';
    const expanded=await shot('small-mcp-'+index+'-expanded',f=>f.text.includes('output: VIS-MCP-'+label+'-BEGIN')&&f.text.includes('VIS-MCP-'+label+'-END')&&f.text.includes('available parameter'));
    if(!['[Part preview truncated]','[output preview truncated; full result retained]','[truncated]'].every(text=>expanded.text.includes(text)))
      throw Error('Small preview lost literal payload markers');
    if(!Object.values(draftChecks(expanded)).every(Boolean))throw Error('Small expansion stole draft/caret');
    await click(index);
    await shot('small-mcp-'+index+'-recollapsed',f=>target(f,index).length===1&&!f.text.includes('output: VIS-MCP-'+label));
  }
  const shellClick=async()=>{
    const hits=visibleMatches(await frame(),'$ printf').filter(point=>point.y>0);
    if(hits.length!==1)throw Error('Small Shell header not unique');
    const point=hits[0];send(`\x1b[<0;${point.x+1};${point.y+1}M\x1b[<0;${point.x+1};${point.y+1}m`,'tool_preview_shell_click');
  };
  await shellClick();
  await shot('small-shell-expanded',f=>f.text.split('\n').some(line=>line.trimEnd().endsWith('VIS'))&&!/\(\d+ earlier lines\)/.test(f.text));
  await shellClick();
  await shot('small-shell-recollapsed',f=>/\(\d+ earlier lines\)/.test(f.text));
  for(const [columns,rows] of [[80,24],[160,48],[120,40]]) {
    await resize(rows,columns);
    await shot('small-'+columns+'x'+rows,f=>f.columns===columns&&f.rows===rows&&f.text.includes('preserved draft')&&f.text.includes('VIS16-DONE:'));
  }
  send('\x03','tool_preview_clear_draft');await waitFor(f=>!f.text.includes('preserved draft'),'small draft cleared',6000);
  await slash('/new');await waitFor(f=>f.text.includes('Ask anything'),'small switched',6000);
  await reopen('small-reopened');
  unchanged(before,await snapshot('small-reopened'));
  facts.exits=[await naturalExit(0)];
  await relaunch();
  await waitFor(f=>f.text.includes('Ask anything')||f.text.includes('VIS16-DONE:'),'small restart ready',15000);
  await reopen('small-restarted');
  facts.after=await snapshot('small-restarted');unchanged(before,facts.after);
  facts.exits.push(await naturalExit(1));
  if(requests()!==5||calls()!==3)throw Error('Small final exit created new requests/calls');
  return facts;
}
