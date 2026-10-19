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
  const code=()=>logs.find(event=>event.kind==='fixture_tool_call'&&event.call_id==='call_vis46_success')?.arguments.code;
  const dialogShot=async(stage,predicate)=>{
    const f=await shot(stage,predicate);
    const width=Math.min(116,f.columns-2),left=Math.ceil((f.columns-width)/2);
    const title=visibleMatches(f,'execute').filter(p=>p.x===left+3&&
      f.cells[p.y].slice(p.x).map(c=>c.symbol).join('').startsWith('execute '));
    const footer=visibleMatches(f,'scroll ↑/↓');
    if(title.length!==1||footer.length!==1||title[0].x!==left+3||footer[0].x!==left+3)
      throw Error('Execute dialog inset/title/footer geometry differs');
    for(let y=title[0].y+2;y<=footer[0].y-2;y++)
      for(let x=left;x<left+width;x++)
        if(f.cells[y][x].bg!=='#1e1e1e')throw Error('Execute content band is not raised-high');
    const row=f.cells[footer[0].y];
    for(let x=footer[0].x;x<footer[0].x+10;x++)
      if(row[x].fg!==(x<footer[0].x+6?'#eeeeee':'#808080')||row[x].modifiers.includes('bold'))
        throw Error('Execute footer title/key styling differs');
    return f;
  };
  const clipboard=async(key,expected)=>{
    const offset=readVt().length;
    send(key,'execute_copy_'+key);
    const copied=()=>[...readVt().slice(offset).matchAll(/\x1b\]52;[^;]*;([A-Za-z0-9+/=]*)(?:\x07|\x1b\\)/g)]
      .map(match=>Buffer.from(match[1],'base64').toString('utf8'));
    await waitFor(()=>copied().length>0,'actual OSC52 '+key,6000);
    if(copied().at(-1)!==expected)throw Error('Execute copied bytes differ from available captured '+key);
    await dialogShot('execute-copied-'+key,f=>f.text.includes('copied'));
  };
  const unchanged=(before,after,expectedRequests,expectedCalls)=>{
    for(const key of ['artifacts','mcp_calls','observations','shell_effect','http_fetches'])
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
  await dialogShot('execute-receiving-dialog',f=>f.text.includes('Receiving code')&&f.text.includes('Code'));
  release('release-code');
  await dialogShot('execute-running-dialog',f=>f.text.includes('Running')&&f.text.includes('const first')&&logs.some(event=>event.kind==='execute_fetch'));
  await clipboard('c',code());
  const running=await snapshot('execute-running');
  if(running.mcp_calls.length!==2||logs.filter(event=>event.kind==='execute_fetch').length!==2||
     logs.filter(event=>event.kind==='execute_fetch').some(event=>!event.valid))
    throw Error('Execute did not perform two real MCP calls and distinct direct/nested GETs');
  release('release-fetch');
  await dialogShot('execute-completed-dialog',f=>f.text.includes('Completed')&&f.text.includes('VIS46-RESULT')&&f.text.includes('Output'));
  const result=logs.find(event=>event.kind==='execute_expected_output');
  if(!result)throw Error('Missing structured fixture output contract');
  await clipboard('o',result.text);
  send('\x1b[H','execute_scroll_home');
  await dialogShot('execute-wrapped-code',f=>f.text.includes('const discovered')&&!f.text.includes('←/→'));
  send('\x1b[F','execute_scroll_end');
  await dialogShot('execute-output-end',f=>f.text.includes('VIS46 actual log'));
  for(const [columns,rows] of [[80,24],[160,48],[120,40]]) {
    await resize(rows,columns);
    await dialogShot('execute-dialog-'+columns+'x'+rows,f=>f.columns===columns&&f.rows===rows&&f.text.includes('Completed'));
  }
  send('\x1b','execute_close');
  await shot('execute-completed',f=>f.text.includes('VIS46-DONE')&&f.cursor.visible&&
    !f.text.split('\n').some(line=>line.trim()==='Code'));
  send('\x1b[200~preserved draft\x1b[201~','execute_draft');
  const draft=await waitFor(f=>f.text.includes('preserved draft'),'execute draft',6000);
  const groupLabel='Explored: 1 read, 2 searches, 1 fetch';
  const idle=async()=>{
    send('\x1b[<35;1;40M','execute_hover_out');
    return waitFor(f=>f.text.includes('preserved draft'),'idle tool styles',6000);
  };
  const muted=(f,needle)=>{
    const hits=visibleMatches(f,needle);
    if(hits.length!==1)throw Error('Muted target is not unique: '+needle);
    const p=hits[0];
    for(let x=p.x;x<p.x+needle.length;x++)
      if(f.cells[p.y][x].symbol.trim()&&f.cells[p.y][x].fg!=='#808080')
        throw Error('Idle tool is not muted: '+needle+' at '+x);
    return p;
  };
  let collapsed=await shot('exploration-collapsed',f=>f.text.includes(groupLabel)&&
    !f.text.includes('Read execute-absent.txt')&&!f.text.includes('WebFetch '));
  const header=muted(await idle(),groupLabel);
  clickPoint(header);
  let group=await shot('exploration-expanded',f=>f.text.includes('Read execute-absent.txt')&&
    f.text.includes('WebFetch http://localhost:'));
  const memberNames=['Read execute-absent.txt','Grep "VIS46_MARKER"','Glob "execute-note.txt"','WebFetch http://localhost:'];
  const members=memberNames.map(name=>visibleMatches(group,name));
  if(members.some(hits=>hits.length!==1)||members.some((hits,i)=>i&&hits[0].y!==members[i-1][0].y+1))
    throw Error('Exploration members are not original adjacent rows');
  for(const [i,glyph] of ['→','✱','✱','%'].entries()) {
    const p=members[i][0],row=group.cells[p.y];
    if(row[p.x-2].symbol!==glyph||row[p.x-1].symbol!==' '||row[p.x-1].fg!=='#ffffff'||
       row[p.x].fg!==(i===0?'#e06c75':'#808080'))throw Error('Exploration icon/style/spacer differs');
  }
  clickPoint(members[0][0]);
  group=await shot('exploration-member-error',f=>{
    const read=visibleMatches(f,memberNames[0]),grep=visibleMatches(f,memberNames[1]);
    return read.length===1&&grep.length===1&&grep[0].y>read[0].y+1&&
      f.cells.slice(read[0].y+1,grep[0].y).some(row=>row.some(cell=>cell.symbol.trim()&&cell.fg==='#e06c75'));
  });
  clickPoint(visibleMatches(group,memberNames[0])[0]);
  group=await shot('exploration-error-recollapsed',f=>{
    const read=visibleMatches(f,memberNames[0]),grep=visibleMatches(f,memberNames[1]);
    return read.length===1&&grep.length===1&&grep[0].y===read[0].y+1;
  });
  clickPoint(visibleMatches(group,groupLabel)[0]);
  collapsed=await shot('exploration-recollapsed',f=>f.text.includes(groupLabel)&&!f.text.includes(memberNames[0]));
  for(const [name,expandedText] of [['fetch [method=GET','method:'],['search [query=output','namespace:']]) {
    const before=await idle();
    const p=muted(before,name);
    clickPoint(p);
    const opened=await shot('execute-'+name.split(' ')[0]+'-expanded',f=>f.text.includes(expandedText));
    if(!opened.text.includes('preserved draft')||JSON.stringify(opened.cursor)!==JSON.stringify(draft.cursor))
      throw Error('Nested fetch/search disclosure stole draft/caret');
    const bare=visibleMatches(opened,name.split(' ')[0]).filter(at=>
      opened.cells[at.y].map(cell=>cell.symbol).join('').trim()==='› '+name.split(' ')[0]);
    if(bare.length!==1)throw Error('Expanded nested call is not unique');
    clickPoint(bare[0]);
    await shot('execute-'+name.split(' ')[0]+'-recollapsed',f=>!f.text.includes(expandedText));
  }
  const current=await idle();
  const child=visibleMatches(current,'visexecute.output [index=2');
  if(child.length!==1)throw Error('Execute child is not unique');
  send(`\x1b[<35;${child[0].x+1};${child[0].y+1}M`,'execute_hover');
  await shot('execute-child-hover',f=>JSON.stringify(f.cells)!==JSON.stringify(current.cells));
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
  if(requests()!==3||calls()!==5||completed.mcp_calls.length!==2||completed.http_fetches!==2)throw Error('Execute success request graph differs');
  send('\x03','execute_clear_draft');
  await waitFor(f=>!f.text.includes('preserved draft'),'execute draft cleared',6000);
  send('\x1b[200~VIS46 exercise the supplied failed program exactly once.\x1b[201~','execute_error_prompt');
  send('\r','execute_error_submit');
  await shot('execute-error',f=>f.text.includes('VIS46-ERROR-DONE')&&f.text.includes('VIS46 ERROR'));
  const failedFrame=await frame();
  const previewHead=visibleMatches(failedFrame,'↳ Error: VIS46 ERROR');
  const previewWrap=visibleMatches(failedFrame,'dialog edge.');
  if(previewHead.length!==1||previewWrap.length!==1||previewWrap[0].x!==previewHead[0].x||
     previewWrap[0].y!==previewHead[0].y+1||failedFrame.cells[previewWrap[0].y][previewWrap[0].x].fg!=='#e06c75')
    throw Error('Execute error preview continuation has an extra inset or wrong color');
  const failedParents=parent(failedFrame).filter(point=>
    failedFrame.cells[point.y].map(cell=>cell.symbol).join('').trim()==='✗ execute');
  if(failedParents.length!==1)throw Error('Failed execute parent is not unique');
  clickPoint(failedParents[0]);
  await dialogShot('execute-error-dialog',f=>f.text.includes('Failed')&&!f.cursor.visible&&f.text.includes('Code'));
  send('\x1b[F','execute_error_output_end');
  await dialogShot('execute-error-output',f=>f.text.includes('line f')&&f.text.includes('Failed'));
  send('\x1b','execute_error_dialog_close');
  await shot('execute-error-closed',f=>!f.text.includes('line f')&&f.text.includes('VIS46 ERROR')&&f.cursor.visible);
  const failed=await snapshot('execute-error');
  if(requests()!==5||calls()!==6||failed.mcp_calls.length!==3||failed.http_fetches!==2||logs.some(event=>event.kind==='provider'&&!event.valid))
    throw Error('Execute error request graph differs');
  if(origin==='oc') {
    const operations=failed.observations.flatMap(item=>item.data.tool_operations||[]);
    const records=failed.observations.flatMap(item=>item.data.presentation||[]).filter(record=>record.presentation.execute);
    if(operations.length!==6||operations.filter(op=>op.name==='execute').length!==2||records.length!==2)
      throw Error('Execute created nested durable operations or lost final facts');
    const success=records[0].presentation.execute,error=records[1].presentation.execute;
    if(!success||success.error||JSON.stringify(success.calls.map(call=>call.tool))!==JSON.stringify(['search','visexecute.output','visexecute.output','fetch'])||success.calls.some(call=>call.status!=='completed')||
       !error?.error||error.calls.length!==1||error.calls[0].status!=='error')throw Error('Execute final typed facts differ');
    if(failed.observations.flatMap(item=>item.data.settled_raw||[]).length!==2)
      throw Error('Missing immutable settled RAW evidence');
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
  unchanged(failed,await snapshot('execute-reopened'),5,6);
  const exits=[await exit(0)];
  await relaunch();
  await waitFor(f=>f.text.includes('Ask anything')||f.text.includes('VIS46-ERROR-DONE'),'execute restarted',15000);
  await reopen('execute-restarted');
  const after=await snapshot('execute-restarted');
  unchanged(failed,after,5,6);
  exits.push(await exit(1));
  return {before:failed,after,exits,receiving_follows_running_and_final:true,
    actual_clipboard_code_output:true,exact_request_graph:true,no_tool_replay:true,
    exploration_original_members_and_independent_error:true,nested_fetch_search_muted_disclosures:true,
    error_interaction:'failed_parent_details_dialog',
    error_state_equivalence:false};
}
