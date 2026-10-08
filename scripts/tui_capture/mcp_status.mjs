// Mixed actual MCP inventory; observes complete rows, never seeds renderer state.
export async function probeMcpStatus({origin,send,shot,waitFor,frame,visibleMatches,resize,logs,snapshot}) {
  const labels=['Connected ✓','Disabled ○','Failed !'];
  await waitFor(f=>f.text.includes('Ask anything')&&f.text.includes('1 MCP failed /mcps'),'mixed MCP inventory',15000);
  await waitFor(f=>!f.text.includes('service issue'),'startup toast naturally settled',15000);
  send('\x1b[200~preserve MCP Ω界\x1b[201~','mcp_status_draft');
  const idle=await shot('mcp-status-home',f=>f.text.includes('preserve MCP Ω界'));
  const targets=visibleMatches(idle,'/mcps').filter(p=>idle.cells[p.y][2].symbol==='⊙');
  if(targets.length!==1)throw Error('Mixed Home MCP target not unique');
  const p=targets[0];send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'mcp_status_open');
  const selections=[],geometries=[];
  for(const [columns,rows] of [[120,40],[80,24],[160,48]]) {
    if(columns!==120) {
      await resize(rows,columns);
      send('\x1b[A\x1b[A','mcp_status_first_row');
      await waitFor(f=>{const top=Math.min(...labels.flatMap(label=>visibleMatches(f,label).map(hit=>hit.y)));return f.cells[top]?.some(cell=>cell.bg==='#fab283');},'first mixed MCP row',6000);
    }
    geometries.push({columns,rows});
    for(let index=0;index<3;index++) {
      const f=await shot(`mcp-status-${columns}x${rows}-selection-${index}`,f=>f.text.includes('MCP servers')&&labels.every(label=>f.text.includes(label)));
      for(const name of ['visdisabled','visfailed','vishealthy']) {
        if(visibleMatches(f,name).length!==1)throw Error('Configured MCP label not displayed exactly: '+name);
      }
      const statuses=labels.map(label=>{
        const hits=visibleMatches(f,label);if(hits.length!==1)throw Error('Mixed MCP status not unique: '+label);
        const hit=hits[0];return {label,x:hit.x,y:hit.y,cells:f.cells[hit.y].slice(hit.x,hit.x+[...label].length)};
      });
      selections.push(statuses);
      if(index<2){send('\x1b[B','mcp_status_down');await waitFor(next=>JSON.stringify(next.cells)!==JSON.stringify(f.cells),'next mixed MCP selection',6000);}
    }
  }
  await resize(40,120);
  send('visfailed','mcp_label_search');
  await shot('mcp-status-configured-search',f=>f.text.includes('MCP servers')&&f.text.includes('visfailed')&&f.text.includes('Failed !')&&!f.text.includes('Connected ✓')&&!f.text.includes('Disabled ○'));
  send('\r','mcp_label_details');
  await shot('mcp-status-configured-details',f=>f.text.includes('visfailed')&&!f.text.includes('Failed !')&&!f.text.includes('Connected ✓'));
  send('\x1b','mcp_label_details_close');
  await waitFor(f=>f.text.includes('MCP servers')&&f.text.includes('Failed !'),'MCP label details return to filtered list',6000);
  send('\x1b','mcp_status_close');
  const restored=await shot('mcp-status-restored',f=>f.text.includes('preserve MCP Ω界')&&!f.text.includes('MCP servers'));
  if(JSON.stringify(idle.cursor)!==JSON.stringify(restored.cursor))throw Error('Mixed MCP list changed composer caret');
  const facts=await snapshot('mcp_status_complete');
  if(logs.some(e=>e.kind==='provider'||e.kind==='fixture_tool_call')||facts.mcp_calls.length||facts.shell_effect!==null)throw Error('Mixed MCP observation caused model/tool effects');
  if(!facts.mcp_lifecycle.some(e=>e.phase==='failed')||!facts.mcp_lifecycle.some(e=>e.phase==='healthy'))throw Error('Mixed statuses lack real initialize receipts');
  const expected={'Connected ✓':'#7fd88f','Disabled ○':'#808080','Failed !':'#e06c75'};
  const statuses=selections.flat();
  const semantic_tones=statuses.every(row=>row.cells.every(cell=>cell.fg===(cell.bg==='#fab283'?'#0a0a0a':expected[row.label])));
  const intrinsic_connected_bold=statuses.every(row=>row.cells.every(cell=>cell.modifiers.includes('bold')===(row.label==='Connected ✓')));
  if(!semantic_tones||origin==='oc'&&!intrinsic_connected_bold)throw Error('MCP typed status tone/bold contract failed');
  return {status:'OBSERVED_MCP_STATUS',provider_requests:0,tool_effects:0,draft_and_caret_restored:true,selections,geometries,
    semantic_tones,intrinsic_connected_bold,
    configured_labels:['visdisabled','visfailed','vishealthy'],configured_search_and_details:true,
    source_contract:'Unselected status semantic tones; selected action foreground override; intrinsic Connected bold independent of foreground. Running original may differ from source bold.'};
}
