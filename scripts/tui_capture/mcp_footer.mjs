// Real Select controls and independent stdio initialize counters; no UI seeding.
export async function probeMcpFooter({origin,mode,send,shot,waitFor,frame,visibleMatches,resize,logs,snapshot}) {
  const remap=mode==='remap', next=remap?'\x1bOR':'\x1b[B', prev=remap?'\x1bOQ':'\x1b[A', submit=remap?'\x1bOS':'\r';
  const list=f=>f.text.includes('MCP servers');
  const click=(f,label,event)=>{
    const hits=visibleMatches(f,label);if(hits.length!==1)throw Error('MCP footer target not unique: '+label);
    const p=hits[0];send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,event);
  };
  const paste=(text,event)=>send(`\x1b[200~${text}\x1b[201~`,event);
  let query='';
  const clear=()=>{if(query)send('\x7f'.repeat(query.length),'footer_clear_filter');query='';};
  const filter=async(name,status)=>{
    clear();paste(name,'footer_filter');query=name;
    return waitFor(f=>list(f)&&f.text.includes(name)&&f.text.includes(status),'focused '+name,10000);
  };
  const count=facts=>Object.fromEntries(['healthy-peer','disabled-peer','failed-peer'].map(role=>[role,facts.mcp_lifecycle.filter(e=>e.role===role&&e.method==='initialize').length]));
  const counts=[];
  const checkpoint=async(tag,expected)=>{
    const facts=await snapshot(tag), actual=count(facts);
    if(JSON.stringify(actual)!==JSON.stringify(expected))throw Error('Unexpected actual MCP initialize effects: '+JSON.stringify({tag,actual,expected}));
    if(facts.mcp_calls.length||facts.shell_effect!==null||logs.some(e=>e.kind==='provider'||e.kind==='fixture_tool_call'))throw Error('Select controls caused model/tool effects');
    counts.push({tag,actual});return facts;
  };
  await waitFor(f=>f.text.includes('Ask anything')&&f.text.includes('1 MCP failed /mcps'),'mixed inventory',15000);
  await waitFor(f=>!f.text.includes('service issue'),'natural startup toast settlement',15000);
  paste('footer preserve Ω界','footer_composer');
  const idle=await shot('mcp-footer-home',f=>f.text.includes('footer preserve Ω界'));
  const homeTargets=visibleMatches(idle,'/mcps').filter(p=>idle.cells[p.y][2].symbol==='⊙');
  if(homeTargets.length!==1)throw Error('Home MCP footer not unique');
  const hp=homeTargets[0];send(`\x1b[<0;${hp.x+1};${hp.y+1}M\x1b[<0;${hp.x+1};${hp.y+1}m`,'footer_open');
  await waitFor(list,'actual MCP list',6000);
  const initial={'healthy-peer':1,'disabled-peer':0,'failed-peer':1};
  await checkpoint('footer_initial',initial);
  const focusObservations=[];
  for(const [columns,rows] of [[120,40],[80,24],[160,48]]) {
    await resize(rows,columns);
    await filter('vishealthy','Connected ✓');
    const normal=await shot(`mcp-footer-${columns}x${rows}-row`,f=>list(f)&&f.text.includes('disconnect '+(remap?'f6':'space')));
    send('\t','footer_tab');
    const focused=await shot(`mcp-footer-${columns}x${rows}-focused`,f=>list(f)&&visibleMatches(f,'disconnect').some(p=>f.cells[p.y][p.x].modifiers.includes('bold')));
    const action=visibleMatches(focused,'disconnect')[0], status=visibleMatches(focused,'Connected ✓')[0];
    const title=visibleMatches(focused,'vishealthy').find(p=>p.y===status.y);
    const titleCells=focused.cells[title.y].slice(title.x,title.x+10), actionCells=focused.cells[action.y].slice(action.x,action.x+10);
    if(titleCells.some(c=>c.fg!=='#808080'||c.modifiers.includes('bold'))||actionCells.some(c=>!c.modifiers.includes('bold')))throw Error('Footer logical focus did not mute row/bold action title');
    const shortcut=visibleMatches(focused,remap?'f6':'space').find(p=>p.y===action.y);
    if(focused.cells[shortcut.y].slice(shortcut.x,shortcut.x+(remap?2:5)).some(c=>c.modifiers.includes('bold')))throw Error('Footer shortcut inherited title bold');
    if(JSON.stringify(normal.cursor)!==JSON.stringify(focused.cursor))throw Error('Footer focus stole Search caret');
    focusObservations.push({columns,rows,selected:titleCells,action:actionCells,cursor_preserved:true});
    send('\x1b[Z','footer_shift_tab');
    await shot(`mcp-footer-${columns}x${rows}-unfocused`,f=>list(f)&&visibleMatches(f,'disconnect').every(p=>!f.cells[p.y][p.x].modifiers.includes('bold')));
    send('\t','footer_refocus_before_hover');
    await waitFor(f=>list(f)&&visibleMatches(f,'disconnect').some(p=>f.cells[p.y][p.x].modifiers.includes('bold')),'refocused footer',6000);
    send(`\x1b[<35;${title.x+1};${title.y+1}M`,'footer_row_hover');
    await shot(`mcp-footer-${columns}x${rows}-hover-unfocused`,f=>list(f)&&visibleMatches(f,'disconnect').every(p=>!f.cells[p.y][p.x].modifiers.includes('bold')));
  }
  await resize(40,120);
  await filter('visfailed','Failed !');
  if(remap) {
    const before=await frame();send('\x1b[B\r\x10','footer_disabled_defaults');
    await shot('mcp-footer-old-defaults-inert',f=>list(f)&&JSON.stringify(f.cells)===JSON.stringify(before.cells));
  }
  clear();
  await waitFor(f=>list(f)&&f.text.includes('Connected ✓')&&f.text.includes('Disabled ○')&&f.text.includes('Failed !'),'unfiltered inventory',6000);
  send(next,'footer_effective_next');
  await shot('mcp-footer-effective-next',f=>list(f)&&f.text.includes('retry '+(remap?'f6':'space')));
  send(prev,'footer_effective_previous');
  await shot('mcp-footer-effective-prev',f=>list(f)&&f.text.includes('connect '+(remap?'f6':'space')));
  send(remap?'\x1be':'\x1b[F','footer_effective_end');
  await shot('mcp-footer-effective-end',f=>list(f)&&f.text.includes('disconnect '+(remap?'f6':'space')));
  const selected=f=>['visdisabled','visfailed','vishealthy'].findIndex(name=>visibleMatches(f,name).some(p=>f.cells[p.y][p.x].bg==='#fab283'));
  send(remap?'\x1bu':'\x1b[5~','footer_effective_page_up');
  const pageUp=await shot('mcp-footer-effective-page-up',list), pageUpIndex=selected(pageUp);
  if(pageUpIndex<0||origin==='oc'&&pageUpIndex!==2)throw Error('Native Select -10 did not use source boundary wrap');
  send(remap?'\x1bd':'\x1b[6~','footer_effective_page_down');
  const pageDown=await shot('mcp-footer-effective-page-down',f=>list(f)&&selected(f)===0);
  send(remap?'\x1bh':'\x1b[H','footer_effective_home');
  await shot('mcp-footer-effective-home',f=>list(f)&&f.text.includes('connect '+(remap?'f6':'space')));
  if(!remap) {
    send('\x10','footer_ctrl_p_wrap');
    await shot('mcp-footer-ctrl-p-wrap',f=>list(f)&&f.text.includes('disconnect space'));
    send('\x0e','footer_ctrl_n_wrap');
    await shot('mcp-footer-ctrl-n-wrap',f=>list(f)&&f.text.includes('connect space'));
  }
  await filter('visfailed','Failed !');
  paste('-no-such-row','footer_no_match');
  query+='-no-such-row';
  await shot('mcp-footer-no-match',f=>list(f)&&f.text.includes('No results found')&&f.text.includes('retry '+(remap?'f6':'space')));
  send('\t'+submit,'footer_disabled_submit');
  await waitFor(f=>list(f)&&f.text.includes('No results found'),'disabled action remains list',6000);
  await checkpoint('footer_no_match_no_effect',initial);
  await filter('vishealthy','Connected ✓');send('\t'+submit,'footer_disconnect_focused');
  await shot('mcp-footer-disconnected',f=>list(f)&&f.text.includes('vishealthy')&&f.text.includes('Disabled ○')&&f.text.includes('connect '+(remap?'f6':'space')));
  await checkpoint('footer_disconnect',initial);
  click(await frame(),'connect','footer_mouse_connect');
  await shot('mcp-footer-reconnected',f=>list(f)&&f.text.includes('vishealthy')&&f.text.includes('Connected ✓'));
  const reconnected={...initial,'healthy-peer':2};await checkpoint('footer_mouse_reconnect',reconnected);
  await filter('visfailed','Failed !');
  send(remap?'\x1b[17~':' ','footer_effective_retry');
  const retryDeadline=Date.now()+15000;
  for(let poll=0;;poll++) {
    if(count(await snapshot('footer_retry_poll_'+poll))['failed-peer']===2)break;
    if(Date.now()>=retryDeadline)throw Error('Missing actual failed retry initialize');
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  await shot('mcp-footer-retried',list);
  const retried={...reconnected,'failed-peer':2};await checkpoint('footer_retry',retried);
  await filter('visdisabled','Disabled ○');
  send(remap?'\x07t':' ','footer_chord_or_space_connect');
  await shot('mcp-footer-disabled-activated',f=>list(f)&&f.text.includes('visdisabled')&&f.text.includes('Connected ✓'));
  const activated={...retried,'disabled-peer':1};await checkpoint('footer_explicit_disabled_activation',activated);
  click(await frame(),'disconnect','footer_mouse_disable');
  await waitFor(f=>list(f)&&f.text.includes('Disabled ○'),'explicit disabled park',10000);
  await checkpoint('footer_disabled_park',activated);
  send('\x1b','footer_close');
  const restored=await shot('mcp-footer-restored',f=>f.text.includes('footer preserve Ω界')&&!f.text.includes('MCP servers'));
  if(JSON.stringify(idle.cursor)!==JSON.stringify(restored.cursor))throw Error('MCP footer controls changed composer caret');
  return {status:'OBSERVED_MCP_FOOTER',mode,focusObservations,initialize_counts:counts,
    page_selection_policy:{source:'boundary wrap, not modulo or clamp',page_up_from_last:pageUpIndex,page_down:selected(pageDown),native_source_verified:origin==='oc',running_reference_difference:origin==='upstream'&&pageUpIndex!==2},
    provider_requests:0,tool_effects:0,disabled_initially_not_started:true,explicit_controls:true,
    effective_dispatch_and_hint:true,draft_and_caret_restored:true};
}
