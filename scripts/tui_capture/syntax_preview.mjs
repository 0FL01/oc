// Actual ordinary write/stream/replay through each running binary, not a token UI.
import fs from 'node:fs';

export async function probeSyntax({origin,send,shot,waitFor,frame,visibleMatches,resize,logs,snapshot,control,relaunch}) {
  const fixtures=JSON.parse(fs.readFileSync(new URL('../../crates/oc-tui/assets/syntax/fixtures.json',import.meta.url)));
  const languages=Object.keys(fixtures);
  const prompt='VIS14 syntax proof: write the owned file, then show the full grammar inventory.';
  const checks={languages,stages:[],no_replay:false};
  const rootDraft='syntax parked root draft';
  const click=(point,label)=>send(`\x1b[<0;${point.x+1};${point.y+1}M\x1b[<0;${point.x+1};${point.y+1}m`,label);
  const root=f=>visibleMatches(f,'Syntax inventory fixture').filter(point=>point.y===0);
  const home=f=>f.text.includes('Ask anything')&&f.text.includes('New session')&&!f.text.includes('GRAMMAR::rust');
  const add=async()=> {
    const f=await frame(), targets=visibleMatches(f,' + ').filter(point=>point.y===0);
    if(targets.length!==1||root(f).length!==1)throw Error('Busy root did not expose one actual actionable Add target');
    click({x:targets[0].x+1,y:0},'syntax_busy_root_add');
    await waitFor(home,'actual busy-root Home',6000);
  };
  const ready=f=>f.text.includes('SYNTAX-INVENTORY-DONE')&&languages.every(name=>f.text.includes('GRAMMAR::'+name));
  const rgb=f=> {
    const row=f.cells.find(row=>row.map(cell=>cell.symbol).join('').includes(prompt));
    if(!row||row.find(cell=>cell.symbol==='┃')?.fg!=='#12ab34')throw Error('Accepted user stripe did not follow actual current profile');
  };
  const userHover=(f,expected)=> {
    const y=f.cells.findIndex(row=>row.map(cell=>cell.symbol).join('').includes(prompt));
    const x=f.cells[y]?.findIndex(cell=>cell.symbol==='┃');
    if(y<1||y+1>=f.cells.length||x<0||x+1>=f.cells[y].length-2||[y-1,y,y+1].some(row=>
      f.cells[row].slice(x+1,f.cells[row].length-2).some(cell=>cell.bg!==expected))) {
      throw Error('Accepted user hover did not follow the actual pointer/resize or fresh replay mount');
    }
  };
  const writeBody=f=> {
    const rows=f.cells.map(row=>row.map(cell=>cell.symbol).join(''));
    const header=rows.findIndex(row=>row.includes('# Wrote')&&row.includes('syntax-proof.rs'));
    const content='/* café\n中文 😀 */\nfn main() {\n    let value = "世界";\n    println!("{}", value);\n}\n';
    if(header<0||content.split('\n').some((line,index)=>
      rows[header+2+index]?.indexOf(String(index+1).padStart(2,' ')+' '+line)!==5)) {
      throw Error('Actual completed Write did not paint its captured numbered postimage, including the trailing blank line');
    }
    if(rows.slice(header,header+10).some(row=>row.includes('Write content preview')))throw Error('Small actual Write preview was missing/limited');
    for(let index=0;index<7;index++) {
      if([5,7].some(x=>f.cells[header+2+index][x].symbol!==' '||f.cells[header+2+index][x].fg!=='#ffffff')) {
        throw Error('Actual Write gutter padding did not inherit the terminal foreground');
      }
    }
  };
  await waitFor(f=>f.text.includes('Ask anything'),'syntax Home',15000);
  send('\x1b[200~'+prompt+'\x1b[201~','syntax_submit_draft');send('\r','syntax_submit');
  await shot('syntax-live-partial',f=>f.text.includes('GRAMMAR::rust')&&f.text.includes('/* café')&&!f.text.includes('SYNTAX-INVENTORY-DONE'));
  const parkedRequests=logs.filter(item=>item.kind==='provider').length;
  send('\x1b[200~'+rootDraft+'\x1b[201~','syntax_root_draft');
  await waitFor(f=>f.text.includes(rootDraft),'actual root draft',6000);
  await add();
  const selected=await frame(), targets=root(selected);
  if(targets.length!==1||selected.text.includes(rootDraft))throw Error('Home did not keep the root/draft ownership separate');
  click(targets[0],'syntax_busy_root_return');
  await waitFor(f=>f.text.includes(rootDraft)&&f.text.includes('GRAMMAR::rust')&&!ready(f),'same still-busy root and draft',6000);
  await add();
  await shot('syntax-root-parked-home',home);
  if(logs.filter(item=>item.kind==='provider').length!==parkedRequests)throw Error('Root tab interactions generated a provider request');
  control({kind:'tool_preview_control',action:'release-syntax',request_id:'syntax-release'});
  const releaseDeadline=Date.now()+6000;
  while(!logs.some(item=>item.kind==='tool_preview_control_ack'&&item.request_id==='syntax-release'&&item.released)) {
    if(Date.now()>releaseDeadline)throw Error('Owned syntax stream release was not acknowledged');
    await new Promise(resolve=>setTimeout(resolve,25));
  }
  await waitFor(f=>home(f)&&logs.some(item=>item.kind==='provider_completed'&&item.operation==='syntax_inventory'),
    'owned generation completed while root remained parked',6000);
  send('\x18','syntax_home_close_leader');await new Promise(resolve=>setTimeout(resolve,100));send('w','syntax_home_close_key');
  await waitFor(f=>f.text.includes(rootDraft)&&f.text.includes('SYNTAX-INVENTORY-DONE')&&!f.text.includes('New session'),'completed original root and retained draft',6000);
  send('\x03','syntax_root_clear_draft');await waitFor(f=>!f.text.includes(rootDraft)&&f.text.includes('SYNTAX-INVENTORY-DONE'),'root draft explicitly cleared',6000);
  send('\x1b[<35;11;4M','syntax_leave_tab_hover');
  checks.root_add={actual_busy_mouse_add:true,busy_return_draft_preserved:true,
    completed_while_parked:true,home_closed_without_root_effect:true,no_provider_replay:true};
  await resize(320);
   const complete=await shot('syntax-complete-inventory',ready);rgb(complete);writeBody(complete);userHover(complete,'#1e1e1e');
   checks.user_hover_after_resize=true;
  if(complete.text.includes('syntax highlighting preview limited')||complete.text.includes('syntax highlighting asset unavailable'))throw Error('Known grammar failed/limited in actual TUI');
  const before=await snapshot('syntax-completed');checks.before=before;
  const expected='/* café\n中文 😀 */\nfn main() {\n    let value = "世界";\n    println!("{}", value);\n}\n';
  if(before.syntax?.file?.content!==expected||logs.filter(e=>e.kind==='fixture_tool_call').length!==1||logs.some(e=>e.kind==='provider'&&!e.valid))throw Error('Actual owned write effect/call failed');
  if(origin==='upstream') {
    const frozen=logs.find(e=>e.kind==='frozen_syntax_cache');
    if(!frozen)throw Error('Actual original did not receive frozen syntax inputs');
    const receipt=frozen;
    const usage=new Map(before.syntax.cache_usage.map(item=>[item.file,item]));
    const required=receipt.files.filter(item=>item.kind==='grammar'||item.kind==='highlights');
    if(required.filter(item=>item.kind==='grammar').length!==34||required.some(item=>usage.get(item.file)?.sha256!==item.sha256||!(usage.get(item.file)?.atime_ns>item.initial_atime_ns)))throw Error('Actual original worker did not read every frozen grammar/highlight input');
    checks.frozen_reference_consumed=required.length;
  } else {
    const operations=before.observations.flatMap(item=>item.data.tool_operations||[]);
    if(operations.length!==1||operations[0].name!=='write'||operations[0].state!=='completed')throw Error('Native actual write operation was not completed');
  }
  send('\x1b[200~syntax unsent draft\x1b[201~','syntax_unsent_draft');
  await resize(24,80);
  const narrow=await shot('syntax-current-profile-draft',f=>f.text.includes('syntax unsent draft'));
  const narrowRows=narrow.cells.map(row=>row.map(cell=>cell.symbol).join(''));
  const expectedWrap=[
    '**strong 中文 😀** and *emphasis* with `literal` and [link](https://',
    'example.invalid).',
    'pub fn main() void { const value: u32 = 42; std.debug.print("{d}", .{',
    'value}); }',
  ];
  if(expectedWrap.some(source=>!narrowRows.some(row=>row.includes(source))))throw Error('Actual narrow Markdown/Zig source did not follow pinned word boundaries');
  checks.narrow_source_wrap=true;
  send('\x03','syntax_clear_draft');await waitFor(f=>!f.text.includes('syntax unsent draft'),'syntax draft cleared',6000);
  send('\x03','syntax_natural_exit');
  const deadline=Date.now()+6000;while(!logs.some(item=>item.kind==='exit'&&item.generation===0)) {if(Date.now()>deadline)throw Error('Syntax process did not exit');await new Promise(resolve=>setTimeout(resolve,25));}
  await relaunch();await resize(40,120);
  await waitFor(f=>f.text.includes('Ask anything')||f.text.includes('SYNTAX-INVENTORY-DONE'),'syntax restart',15000);
  if(!(await frame()).text.includes('SYNTAX-INVENTORY-DONE')) {
    send('/sessions','syntax_sessions');await waitFor(f=>f.text.includes('/sessions'),'syntax sessions draft',6000);send('\r','syntax_sessions_enter');
    const list=await waitFor(f=>f.text.includes('Sessions')&&f.text.includes('Syntax inventory fixture'),'syntax saved session',6000);
    // The same actual title also remains in the Root tab above the modal.
    // Select only its painted session-list occurrence below the modal header.
    const heading=list.cells.findIndex(row=>row.map(cell=>cell.symbol).join('').includes('Sessions'));
    const hits=visibleMatches(list,'Syntax inventory fixture').filter(hit=>hit.y>heading);
    if(heading<0||hits.length!==1)throw Error('Syntax saved session not unique in its actual modal');
     const p=hits[0];send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'syntax_reopen');
     await waitFor(f=>!f.text.includes('Sessions')&&(f.text.includes('GRAMMAR::')||f.text.includes('SYNTAX-INVENTORY-DONE')),'syntax actual reopened session',6000);
   }
   await resize(320,120);
   // Observe the actual full restored document once before starting the final
   // unchanged four-identical-frame gate. A fresh process lazily compiles the
   // trusted queries for all 39 visible languages; this is not a paint request.
   await frame();
    const replay=await shot('syntax-replayed-inventory',ready);rgb(replay);writeBody(replay);userHover(replay,'#141414');
   checks.replay_user_unhovered=true;
  const after=await snapshot('syntax-restarted');checks.after=after;
  if(JSON.stringify(before.syntax.file)!==JSON.stringify(after.syntax.file)||JSON.stringify(before.observations)!==JSON.stringify(after.observations)||logs.filter(e=>e.kind==='fixture_tool_call').length!==1)throw Error('Syntax reopen replayed effect or changed actual operation/RAW');
  await resize(40,120);
  send('\x1b[200~VIS root cancel: keep the original root and stop only this generation.\x1b[201~','syntax_cancel_draft');
  await waitFor(f=>f.text.includes('VIS root cancel:'),'actual cancel-turn draft',6000);send('\r','syntax_cancel_submit');
  await shot('syntax-root-cancel-pending',f=>f.text.includes('ROOT-CANCEL-READY')&&f.text.includes('esc interrupt'));
  send('\x1b','syntax_cancel_first_escape');await new Promise(resolve=>setTimeout(resolve,100));send('\x1b','syntax_cancel_second_escape');
  await shot('syntax-root-cancelled',f=>f.text.includes('ROOT-CANCEL-READY')&&!f.text.includes('esc interrupt')&&root(f).length===1);
  const cancelled=await snapshot('syntax-cancelled');checks.cancelled=cancelled;
  if(logs.filter(item=>item.kind==='provider'&&item.operation==='root_cancel').length!==1||
     !logs.some(item=>item.kind==='root_cancel_disconnected')||
     JSON.stringify(before.syntax.file)!==JSON.stringify(cancelled.syntax.file)||
     logs.filter(item=>item.kind==='fixture_tool_call').length!==1)throw Error('Addressed root cancellation replayed effects or did not stop the owned provider');
  if(origin==='oc') {
    const turns=cancelled.observations.flatMap(item=>item.data.syntax_turns||[]);
    if(turns.length!==2||turns[0].status!=='completed'||turns[1].status!=='cancelled'||turns[0].session_id!==turns[1].session_id)throw Error('Native cancel did not settle exactly the new turn in the retained root');
    checks.root_add.cancelled_exact_retained_root=true;
  }
   checks.requests=logs.filter(item=>item.kind==='provider').length;checks.no_replay=true;return checks;
}
