// Real PTY pointer/edit proof; no editor state injection or renderer substitute.
const EDITED='Проведи RECXON, жду план', TITLE='VIS prompt caret fixture';
const glyphText=f=>f.cells.map(row=>row.filter(c=>c.width!==0).map(c=>c.symbol).join('')).join('\n');
export async function probePromptCaret({origin,send,shot,waitFor,frame,visibleMatches,resize,logs,snapshot,relaunch}) {
  const observations=[];
  const requests=()=>logs.filter(e=>e.kind==='provider');
  const click=(p,event)=>send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,event);
  const unique=(f,text)=>{const hits=visibleMatches(f,text);if(hits.length!==1)throw Error('Prompt target not unique '+text);return hits[0];};
  const clear=async()=>{
    const before=await frame(), prompt=visibleMatches(before,'Build ·').sort((a,b)=>a.y-b.y).at(-1);
    if(!prompt)throw Error('No painted composer metadata');
    send('\x03','caret_clear');
    await waitFor(f=>f.cursor.x===prompt.x&&(f.text.includes('Ask anything')||
      f.cells[f.cursor.y].slice(prompt.x).every(c=>!c.symbol||c.symbol===' ')),'empty composer',6000);
  };
  const load=async text=>{
    send(`\x1b[200~${text}\x1b[201~`,'caret_fixture_paste');
    return waitFor(f=>glyphText(f).includes(text.split('\n')[0].slice(0,8))||f.text.includes('[Pasted'),'painted fixture draft',6000);
  };
  const editedCase=async(stage,text,anchor,dx,dy,expected)=>{
    const before=await load(text), p=unique(before,anchor);
    const target={x:p.x+dx,y:p.y+dy};
    send(`\x1b[<35;${target.x+1};${target.y+1}M`,'caret_hover');
    const hover=await frame();
    if(JSON.stringify(hover.cursor)!==JSON.stringify(before.cursor))throw Error('Hover moved prompt caret');
    click(target,'caret_left_click');
    const positioned=await shot(stage+'-clicked',f=>glyphText(f).includes(anchor)&&f.cursor.visible&&f.cursor.y===target.y&&(stage==='caret-trailing'||f.cursor.x!==before.cursor.x));
    send('X','caret_type_after_click');
    const after=await shot(stage+'-edited',f=>glyphText(f).includes(expected));
    observations.push({stage,target,before:before.cursor,clicked:positioned.cursor,edited:after.cursor,expected});
    if(requests().length)throw Error('Prompt click/edit submitted a request');
    await clear();
  };
  await shot('caret-home',f=>f.text.includes('Ask anything'));
  await editedCase('caret-home-word','Проведи RECON, жду план','RECON',3,0,EDITED);
  // Unicode11 VT painting can lose the second component of a ZWJ glyph when
  // both applications place the following tail. Observe the unchanged full
  // raster; assert the insertion prefix, not a fabricated normalized emoji.
  await editedCase('caret-wide-second','UNI α界e\u0301👩\u200d💻 tail','UNI ',6,0,'UNI αX界e\u0301');
  await editedCase('caret-combining','UNI α界e\u0301👩\u200d💻 tail','UNI ',7,0,'UNI α界Xe\u0301');
  await editedCase('caret-zwj-start','UNI α界e\u0301👩\u200d💻 tail','UNI ',8,0,'UNI α界e\u0301X');
  await editedCase('caret-trailing','TAIL','TAIL',40,0,'TAILX');
  const wrapped='a'.repeat(65)+' LONGWORD';
  await editedCase('caret-wrap-gap',wrapped,'LONGWORD',69,-1,'a'.repeat(65)+'X');
  let multiline=await load('FIRST\n\nLAST');
  if(multiline.text.includes('[Pasted')&&!glyphText(multiline).includes('FIRST')) {
    click(unique(multiline,'[Pasted'),'caret_expand_multiline');
    multiline=await waitFor(f=>glyphText(f).includes('FIRST')&&glyphText(f).includes('LAST'),'expanded multiline',6000);
  }
  const first=unique(multiline,'FIRST');click({x:first.x+30,y:first.y+1},'caret_blank_line');send('X','caret_blank_type');
  await shot('caret-blank-line',f=>f.text.includes('FIRST')&&f.cells[first.y+1]?.[first.x]?.symbol==='X');
  await clear();
  await resize(40,80);await waitFor(f=>f.columns===80,'80-column prompt',6000);
  await editedCase('caret-resized-word','Проведи RECON, жду план','RECON',3,0,EDITED);
  await resize(40,120);await waitFor(f=>f.columns===120,'120-column prompt',6000);
  send('NEIGHBOR ','caret_chip_prefix');send('\x1b[200~'+('z'.repeat(1600))+'\x1b[201~','caret_collapsed_payload');
  const chip=await waitFor(f=>f.text.includes('NEIGHBOR')&&f.text.includes('[Pasted'),'collapsed paste chip',6000);
  const neighbor=unique(chip,'NEIGHBOR');click({x:neighbor.x+4,y:neighbor.y},'caret_chip_neighbor');send('X','caret_neighbor_type');
  const beside=await shot('caret-chip-neighbor',f=>f.text.includes('NEIGXHBOR')&&f.text.includes('[Pasted'));
  click(unique(beside,'[Pasted'),'caret_chip_priority');
  await shot('caret-chip-expanded',f=>f.text.includes('zzzzzzzz')&&!f.text.includes('[Pasted'));
  await clear();
  // A real old text cell beyond the 60-column command surface belongs to its
  // backdrop while open. Do not click an actual command row/action by accident.
  const blockedText='BLOCK '+('b'.repeat(56))+' RECON';
  const blocked=await load(blockedText);send('\x10','caret_commands_overlay');
  await shot('caret-overlay',f=>f.text.includes('Commands'));
  const covered=unique(blocked,'RECON');click({x:covered.x+4,y:covered.y},'caret_overlay_old_prompt_hit');
  const under=await waitFor(f=>f.text.includes('Commands')||glyphText(f).includes(blockedText),'overlay owns pointer',6000);
  if(under.text.includes('Commands'))send('\x1b','caret_close_overlay');
  const restored=await shot('caret-overlay-restored',f=>glyphText(f).includes(blockedText)&&!f.text.includes('Commands'));
  if(JSON.stringify(restored.cursor)!==JSON.stringify(blocked.cursor))throw Error('Overlay click placed underlying prompt caret');
  await clear();
  const unedited=await load('Проведи RECON, жду план'), word=unique(unedited,'RECON');
  click({x:word.x+3,y:word.y},'caret_final_edit');send('X','caret_final_type');
  await shot('caret-before-submit',f=>glyphText(f).includes(EDITED));
  if(requests().length)throw Error('Mouse or editing dispatched before explicit Enter');
  const empty=await snapshot('caret_before_submit');
  if(empty.mcp_calls.length||empty.shell_effect!==null)throw Error('Editing caused tool effects');
  send('\r','caret_one_explicit_submit');
  await shot('caret-submitted',f=>f.text.includes('VIS-CARET-DONE:'));
  await waitFor(()=>requests().length===2,'one main request plus title',15000);
  const main=requests().filter(e=>e.operation==='prompt_caret_submit');
  if(main.length!==1||!main[0].valid||JSON.stringify(main[0].user_texts)!==JSON.stringify([EDITED])||requests().some(e=>!e.valid))throw Error('Provider did not receive exact edited bytes once');
  const before=await snapshot('caret_accepted');
  const raw=before.observations.flatMap(o=>o.data.prompt_user_messages||[]);
  if(origin==='oc'&&(raw.length!==1||raw[0].text!==EDITED))throw Error('Native durable user row differs from the edit');
  // Session prompt uses different padding/width from Home; no second submit.
  send('\x1b[200~SESSION RECON tail\x1b[201~','caret_session_draft');
  const session=await waitFor(f=>f.text.includes('SESSION RECON tail'),'session composer',6000), sw=unique(session,'SESSION RECON');
  click({x:sw.x+11,y:sw.y},'caret_session_click');send('X','caret_session_type');
  await shot('caret-session-edited',f=>f.text.includes('SESSION RECXON tail'));
  await clear();
  const slash=async text=>{send(text,'caret_command');await waitFor(f=>f.text.includes(text),'command draft',6000);send('\r','caret_command_enter');};
  const reopen=async stage=>{
    await slash('/sessions');const f=await waitFor(f=>f.text.includes('Sessions')&&f.text.includes(TITLE),'saved caret session',6000);
    const hits=visibleMatches(f,TITLE).filter(p=>p.y>0);
    if(hits.length!==1)throw Error('Saved caret session row not unique');
    click(hits[0],'caret_reopen');return shot(stage,f=>f.text.includes('VIS-CARET-DONE:')&&glyphText(f).includes(EDITED));
  };
  await slash('/new');await waitFor(f=>f.text.includes('Ask anything'),'new Home',6000);await reopen('caret-reopened');
  const after=await snapshot('caret_reopened');
  send('\x03','caret_clean_exit');
  const end=Date.now()+6000;while(!logs.some(e=>e.kind==='exit'&&e.generation===0)&&Date.now()<end)await new Promise(r=>setTimeout(r,25));
  if(!logs.some(e=>e.kind==='exit'&&e.generation===0))throw Error('No clean owned exit');
  await relaunch();await waitFor(f=>f.text.includes('Ask anything')||f.text.includes('VIS-CARET-DONE:'),'restart ready',15000);await reopen('caret-restarted');
  const restarted=await snapshot('caret_restarted');
  if(requests().length!==2||[after,restarted].some(s=>s.mcp_calls.length||s.shell_effect!==null||JSON.stringify(s.observations)!==JSON.stringify(before.observations)))throw Error('Reopen/restart replayed or changed accepted RAW');
  return {status:'OBSERVED_PROMPT_CARET',observations,before,after,restarted,provider_requests:2,explicit_user_submissions:1,
    user_text:EDITED,native_durable_user_verified:origin==='oc',tool_effects:0,reopen_restart_no_replay:true};
}
