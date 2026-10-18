// Actual root/child completion, canonical reload and cold reopen. No painted data.
export async function probeHistoryCompletion({origin,send,shot,waitFor,frame,logs,snapshot,control,resize,relaunch,slash,visibleMatches}) {
  const providerCount=()=>logs.filter(event=>event.kind==='provider').length;
  const callCount=()=>logs.filter(event=>event.kind==='fixture_tool_call').length;
  const completed=source=>logs.filter(event=>event.kind==='provider_completed'&&event.operation===source).length;
  const populated=f=>['history-note.txt','T70_MARKER','history-*.txt','history-absent.txt'].every(value=>f.text.includes(value));
  const visual=[];
  const toolAppearance=(stage,f,source)=>{
    const labels=['Read history-note.txt','Grep "T70_MARKER" in . (1 match)',
      'Glob "history-*.txt" in . (1 match)','Read history-absent.txt'];
    const points=labels.map(label=>{
      const hits=visibleMatches(f,label);
      if(hits.length!==1)throw Error('Actual canonical tool label not unique: '+label);
      return hits[0];
    });
    for(let n=0;n<points.length;n++) {
      const {x,y}=points[n];
      if(x!==7||y!==points[0].y+n)throw Error('Exploration member geometry has a gap or changed padding');
      const glyph=f.cells[y][x-2],spacer=f.cells[y][x-1];
      const color=n===3?'#e06c75':'#808080';
       if(glyph.symbol!==(n===1||n===2?'✱':'→')||glyph.fg!==color||spacer.fg!=='#ffffff'||glyph.modifiers.length)
        throw Error('Exploration icon/style differs from pinned file-tool component');
       if(f.cells[y].slice(x,x+labels[n].length).some(c=>c.fg!==color||c.bg!=='#0a0a0a'||c.modifiers.length))
        throw Error('Exploration label color differs from pinned component');
    }
    const group=visibleMatches(f,'Explored — 2 reads, 2 searches');
    if(group.length!==1||group[0].y!==points[0].y-1)throw Error('Terminal exploration grouping differs');
    const final=visibleMatches(f,'T70-'+source.toUpperCase()+'-DONE');
    if(final.length!==1||final[0].y!==points[3].y+2)throw Error('Final text or its single margin changed');
    if(f.cursor.visible!==(source==='root'))throw Error('Prompt/composer cursor visibility differs');
    if(source==='root'&&(f.cursor.x!==5||f.cursor.y!==f.rows-6||f.cursor.shape!=='block'))
      throw Error('Actual root prompt cursor geometry differs');
    visual.push({stage,columns:f.columns,rows:f.rows,group:group[0],members:points,final:final[0],cursor:f.cursor,
      glyphs:points.map(({x,y})=>f.cells[y][x-2]),group_style:f.cells[group[0].y][group[0].x]});
  };
  const errorVisible=f=>{
    const hits=visibleMatches(f,'Read history-absent.txt');
    return hits.length===1&&f.cells[hits[0].y+1]?.some(c=>c.symbol.trim()&&c.fg==='#e06c75');
  };
  const clickError=async offset=>{
    const hits=visibleMatches(await frame(),'Read history-absent.txt');
    if(hits.length!==1)throw Error('Real failed tool header is not unique');
    const {x,y}=hits[0];
    send(`\x1b[<0;${x+offset+1};${y+1}M\x1b[<0;${x+offset+1};${y+1}m`,'history_toggle_real_error');
  };
  const release=async source=>{
    const request_id='history-release-'+source;
    control({kind:'tool_preview_control',request_id,action:'release-'+source});
    await waitFor(()=>logs.some(event=>event.kind==='tool_preview_control_ack'&&event.request_id===request_id),'actual completion release',6000);
  };
  const wheel=up=>send((`\x1b[<${up?64:65};100;10M`).repeat(300),'history_scroll_'+(up?'top':'tail'));
  const thought=async()=>{
    const hits=visibleMatches(await frame(),'Thought');
    if(hits.length!==1)throw Error('Actual reasoning header is not unique');
    send(`\x1b[<0;${hits[0].x+2};${hits[0].y+1}M\x1b[<0;${hits[0].x+2};${hits[0].y+1}m`,'history_toggle_public_reasoning');
  };
  const expanded=async()=>{
    let f=await frame();
    if(populated(f))return;
    const label=f.text.includes('Explored')?'Explored':'Exploring';
    const hits=visibleMatches(f,label);
    if(hits.length!==1)throw Error('Expected one actual exploration header');
    const point=hits[0];
    send(`\x1b[<0;${point.x+1};${point.y+1}M\x1b[<0;${point.x+1};${point.y+1}m`,'history_expand_real_group');
    await waitFor(populated,'all original tool parameters in expanded group',6000);
  };
  const openChild=async()=>{
    const f=await waitFor(f=>f.text.includes('Helper Subagent'),'actual captured child card',6000);
    const hits=visibleMatches(f,'Helper Subagent');
    if(hits.length!==1)throw Error('Captured child heading is not unique');
    const point=hits[0];
    send(`\x1b[<0;${point.x+1};${point.y+1}M\x1b[<0;${point.x+1};${point.y+1}m`,'history_open_original_child');
    await waitFor(f=>f.text.includes('Subagents')&&f.text.includes('Inspect canonical child history.'),'real linked child route',6000);
  };
  const reopenRoot=async()=>{
    await slash('/sessions');
    const f=await waitFor(f=>f.text.includes('Sessions')&&f.text.includes('T70 history inspection'),'saved root picker',6000);
    const hits=visibleMatches(f,'T70 history inspection').filter(point=>point.y>0);
    if(hits.length!==1)throw Error('Saved root title is not unique');
    const point=hits[0];
    send(`\x1b[<0;${point.x+1};${point.y+1}M\x1b[<0;${point.x+1};${point.y+1}m`,'history_reopen_root');
    await waitFor(f=>f.text.includes('T70-PARENT-DONE'),'saved root transcript',6000);
  };

  send('\x1b[200~T70_INSPECT_ROOT\x1b[201~\r','history_root_submit');
  await waitFor(()=>logs.some(event=>event.kind==='completion_ready'&&event.source==='root'),'held root completion',15000);
  await expanded();
  toolAppearance('history-root-live',await shot('history-root-live',populated),'root');
  await release('root');
  await waitFor(f=>completed('root')>=2&&f.text.includes('T70-ROOT-DONE'),'canonical root completion',6000);
  await expanded();
  toolAppearance('history-root-completed',await shot('history-root-completed',f=>populated(f)&&f.text.includes('T70-ROOT-DONE')),'root');
  if(errorVisible(await frame()))throw Error('Failed detail must start collapsed');
  await clickError(1);
  await shot('history-root-error-expanded',errorVisible);
  await clickError(2);
  toolAppearance('history-root-error-recollapsed',await shot('history-root-error-recollapsed',f=>populated(f)&&!errorVisible(f)),'root');
  await thought();
  wheel(false);
  await shot('history-root-reasoning-tail',f=>f.text.includes('T70-THOUGHT-END-4')&&populated(f));
  wheel(true);
  await waitFor(f=>f.text.includes('T70_INSPECT_ROOT')&&f.text.includes('Thought'),'original reasoning header',6000);
  await thought();
  await waitFor(populated,'recollapsed public reasoning',6000);
  send('\x1b[200~T70_INSPECT_CHILD\x1b[201~\r','history_root_delegate');
  await waitFor(()=>logs.some(event=>event.kind==='completion_ready'&&event.source==='child'),'held real child completion',15000);
  await openChild();
  await expanded();
  toolAppearance('history-child-live',await shot('history-child-live',populated),'child');
  await release('child');
  await waitFor(f=>completed('child')>=2&&f.text.includes('T70-CHILD-DONE'),'canonical selected child completion',6000);
  await expanded();
  toolAppearance('history-child-completed',await shot('history-child-completed',f=>populated(f)&&f.text.includes('T70-CHILD-DONE')),'child');
  const before=await snapshot('history-settled');
  if(before.composer.canonical_reasoning_bytes_per_source<=65536)throw Error('Fixture did not exceed old aggregate cap');
  const turns=s=>s.composer.observations.flatMap(o=>o.data.turns??[]);
  if(origin==='oc') {
    const publicTurns=turns(before).filter(t=>t.public_reasoning_sha256.length);
    if(publicTurns.length!==2||publicTurns.some(t=>t.status!=='completed'||
       t.public_reasoning_bytes!==before.composer.canonical_reasoning_bytes_per_source||
       JSON.stringify(t.public_reasoning_sha256)!==JSON.stringify(before.composer.expected_public_reasoning_sha256)))
      throw Error('Native canonical public reasoning was lost or not completed');
  }
  await resize(24,80);
  await shot('history-child-narrow',f=>f.columns===80&&f.rows===24&&f.text.includes('T70-CHILD-DONE'));
  await resize(40,120);
  send('\x1b','history_return_parent');
  await waitFor(f=>f.text.includes('T70-PARENT-DONE'),'parent settled after original child',6000);
  const localCounts={requests:providerCount(),calls:callCount()};
  await slash('/new');
  await reopenRoot();
  wheel(true);
  await waitFor(f=>f.text.includes('T70_INSPECT_ROOT'),'original root history after reopen',6000);
  await expanded();
  toolAppearance('history-root-reopened',await shot('history-root-reopened',f=>populated(f)&&f.text.includes('T70-ROOT-DONE')),'root');
  wheel(false);
  await waitFor(f=>f.text.includes('Helper Subagent')&&f.text.includes('T70-PARENT-DONE'),'settled root tail',6000);
  await slash('/exit');
  await waitFor(()=>logs.some(event=>event.kind==='exit'&&event.generation===0&&event.termination==='natural'),'natural first process exit',6000);
  await relaunch();
  await waitFor(f=>f.text.includes('Ask anything')||f.text.includes('T70-PARENT-DONE'),'natural process relaunch',10000);
  if(!(await frame()).text.includes('T70-PARENT-DONE'))await reopenRoot();
  await openChild();
  await expanded();
  toolAppearance('history-child-cold-reopened',await shot('history-child-cold-reopened',f=>populated(f)&&f.text.includes('T70-CHILD-DONE')),'child');
  const after=await snapshot('history-cold-reopened');
  if(origin==='oc'&&turns(before).filter(t=>t.status==='completed').some(t=>
      !turns(after).some(a=>a.id===t.id&&a.sha256===t.sha256&&a.status===t.status)))
    throw Error('History navigation changed settled RAW');
  if(providerCount()!==localCounts.requests||callCount()!==localCounts.calls)throw Error('History navigation replayed a generation or tool');
  if(logs.some(event=>event.kind==='provider'&&!event.valid))throw Error('Provider graph violated fixture contract');
  if(localCounts.calls!==9)throw Error('Expected eight real file calls and one original delegation');
  return {origin,status:'OBSERVED_HISTORY_COMPLETION',before,after,no_effect_replay:true,
    actual_requests:localCounts.requests,actual_tool_calls:localCounts.calls,
    canonical_reasoning_bytes_per_source:before.composer.canonical_reasoning_bytes_per_source,visual};
}
