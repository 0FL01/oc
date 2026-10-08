// Real Home/footer controls on both binaries; no model calls or seeded UI state.
export async function probeHomeMcp({send,shot,waitFor,frame,visibleMatches,resize,logs,control,snapshot}) {
  const requestCount=()=>logs.filter(e=>e.kind==='provider').length;
  const before=requestCount();
  const transition=async action=>{
    const request_id='home-mcp-'+action;
    control({kind:'tool_preview_control',action,request_id});
    const deadline=Date.now()+6000;
    while(!logs.some(e=>e.kind==='tool_preview_control_ack'&&e.request_id===request_id)) {
      if(Date.now()>=deadline)throw Error('Timed out owned peer phase '+action);
      await new Promise(r=>setTimeout(r,20));
    }
  };
  const click=async label=>{
    const hits=visibleMatches(await frame(),label);
    if(hits.length!==1)throw Error('Home MCP item is not unique: '+label);
    const p=hits[0];send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'home_mcp_click');
  };
  send('\x1b[200~footer Ω界\x1b[201~','home_mcp_draft');
  const idle=await shot('home-mcp-draft',f=>f.text.includes('footer Ω界')&&f.text.includes('1 MCP /mcps'));
  await click('1 MCP /mcps');
  await shot('home-mcp-list',f=>f.text.includes('MCP servers')&&f.text.includes('Connected ✓'));
  send(' ','home_mcp_disconnect');
  await shot('home-mcp-disabled-list',f=>f.text.includes('MCP servers')&&f.text.includes('Disabled ○'));
  send('\x1b','home_mcp_close');
  const disabled=await shot('home-mcp-disabled',f=>f.text.includes('0 MCP /mcps')&&!f.text.includes('MCP servers'));
  if(!disabled.text.includes('footer Ω界')||JSON.stringify(disabled.cursor)!==JSON.stringify(idle.cursor))throw Error('Disconnect changed draft/caret');
  await transition('fail');
  await click('0 MCP /mcps');
  await waitFor(f=>f.text.includes('MCP servers')&&f.text.includes('Disabled ○'),'disabled control ready',6000);
  send(' ','home_mcp_connect');
  await shot('home-mcp-failed-list',f=>f.text.includes('MCP servers')&&f.text.includes('Failed !'));
  send('\x1b','home_mcp_close');
  // Qualify the settled live footer, not the independent five-second toast
  // expiry crossing the PNG/grid resample. No cells or frames are masked.
  await waitFor(f=>!f.text.includes('service issue'),'failure toast naturally settled',15000);
  await shot('home-mcp-failed',f=>f.text.includes('1 MCP failed /mcps')&&!f.text.includes('MCP servers'));
  const failed=await snapshot('home_mcp_failed');
  if(!failed.mcp_lifecycle.some(e=>e.method==='initialize'&&e.phase==='failed'))throw Error('No real MCP initialization failure');
  await transition('recover');
  await click('1 MCP failed /mcps');
  await waitFor(f=>f.text.includes('MCP servers')&&f.text.includes('Failed !'),'failed control ready',6000);
  send(' ','home_mcp_retry');
  await waitFor(f=>f.text.includes('MCP servers')&&f.text.includes('Connected ✓'),'live reconnection',15000);
  send('\x1b','home_mcp_close');
  const restored=await shot('home-mcp-restored',f=>f.text.includes('1 MCP /mcps')&&!f.text.includes('MCP servers'));
  if(!restored.text.includes('footer Ω界')||JSON.stringify(restored.cursor)!==JSON.stringify(idle.cursor))throw Error('Home footer took draft/caret ownership');
  for(const [columns,rows] of [[44,24],[63,24],[64,24],[43,24],[64,12]]) {
    await resize(rows,columns);
    await shot(`home-mcp-${columns}x${rows}`,f=>f.columns===columns&&f.rows===rows&&
      (rows<16?f.cells[rows-1][2].symbol==='⊙'&&f.text.includes('/mcps'):
        columns<44?!f.text.includes('MCP'):f.text.includes('1 MCP')&&(columns>=64?f.text.includes('1 MCP /mcps'):!f.text.includes('/mcps'))));
  }
  await resize(40,120);
  await shot('home-mcp-resized-back',f=>f.columns===120&&f.rows===40&&f.text.includes('1 MCP /mcps'));
  send('\x03','home_mcp_clear_draft');
  await waitFor(f=>!f.text.includes('footer Ω界'),'footer draft cleared',6000);
  if(requestCount()!==before)throw Error('Home MCP controls invoked the model');
  return {status:'PASS_BEHAVIOR_ONLY',provider_requests:requestCount()-before,
    real_failed_initialize:true,draft_and_caret_restored:true,
    disclosed_reference_difference:'At64x12 pinned footer spaces overpaint underlying prompt-footer text; full frame remains unmasked.'};
}
