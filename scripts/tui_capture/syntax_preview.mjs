// Actual ordinary write/stream/replay through each running binary, not a token UI.
import fs from 'node:fs';

export async function probeSyntax({origin,send,shot,waitFor,frame,visibleMatches,resize,logs,snapshot,control,relaunch}) {
  const fixtures=JSON.parse(fs.readFileSync(new URL('../../crates/oc-tui/assets/syntax/fixtures.json',import.meta.url)));
  const languages=Object.keys(fixtures);
  const prompt='VIS14 syntax proof: write the owned file, then show the full grammar inventory.';
  const checks={languages,stages:[],no_replay:false};
  const ready=f=>f.text.includes('SYNTAX-INVENTORY-DONE')&&languages.every(name=>f.text.includes('GRAMMAR::'+name));
  const rgb=f=> {
    const row=f.cells.find(row=>row.map(cell=>cell.symbol).join('').includes(prompt));
    if(!row||row.find(cell=>cell.symbol==='┃')?.fg!=='#12ab34')throw Error('Accepted user stripe did not follow actual current profile');
  };
  await waitFor(f=>f.text.includes('Ask anything'),'syntax Home',15000);
  send('\x1b[200~'+prompt+'\x1b[201~','syntax_submit_draft');send('\r','syntax_submit');
  await shot('syntax-live-partial',f=>f.text.includes('GRAMMAR::rust')&&f.text.includes('/* café')&&!f.text.includes('SYNTAX-INVENTORY-DONE'));
  control({kind:'tool_preview_control',action:'release-syntax',request_id:'syntax-release'});
  const releaseDeadline=Date.now()+6000;
  while(!logs.some(item=>item.kind==='tool_preview_control_ack'&&item.request_id==='syntax-release'&&item.released)) {
    if(Date.now()>releaseDeadline)throw Error('Owned syntax stream release was not acknowledged');
    await new Promise(resolve=>setTimeout(resolve,25));
  }
  await resize(320);
  const complete=await shot('syntax-complete-inventory',ready);rgb(complete);
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
  await shot('syntax-current-profile-draft',f=>f.text.includes('syntax unsent draft'));
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
   const replay=await shot('syntax-replayed-inventory',ready);rgb(replay);
  const after=await snapshot('syntax-restarted');checks.after=after;
  if(JSON.stringify(before.syntax.file)!==JSON.stringify(after.syntax.file)||JSON.stringify(before.observations)!==JSON.stringify(after.observations)||logs.filter(e=>e.kind==='fixture_tool_call').length!==1)throw Error('Syntax reopen replayed effect or changed actual operation/RAW');
  checks.requests=logs.filter(item=>item.kind==='provider').length;checks.no_replay=true;return checks;
}
