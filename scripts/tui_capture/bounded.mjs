import fs from 'node:fs';
import path from 'node:path';

export async function probeBounded({mode,origin,dir,send,waitFor,frame,capture,visibleMatches,logs,prompt,resize}) {
  const checks={mode,origin,status:'IN_PROGRESS',stages:[]};
  const save=()=>fs.writeFileSync(path.join(dir,'bounded-checks.json'),JSON.stringify(checks,null,2)+'\n');
  const counts=()=>({transcript:logs.filter(e=>e.kind==='provider' && e.operation==='transcript').length,
    title:logs.filter(e=>e.kind==='provider' && e.operation==='title').length,
    completed:logs.filter(e=>e.kind==='provider_completed').length,
    invalid:logs.filter(e=>e.kind==='provider' && !e.valid).length});
  const shot=async(name,f,predicates)=>{
    const status=await capture(name,f,'CAPTURED_BOUNDED');
    checks.stages.push({name,predicates,counts:counts(),cursor:f.cursor,status});save();
    if(status!=='CAPTURED_BOUNDED' || Object.values(predicates).some(v=>!v)) throw Error('Bounded predicate failed: '+name);
  };
  const submit=()=>{send('\x1b[200~'+prompt+'\x1b[201~','bounded_prompt');send('\r','bounded_submit');};
  try {
    // Select the already-configured lowercase profile through the real dialog,
    // making the successful selection path and its immediate frame observable.
    send('/agents','bounded_agents');
    await waitFor(f=>f.text.includes('/agents'),'agents draft ready');
    send('\r','bounded_agents_submit');
    await waitFor(f=>f.text.includes('Select agent'),'bounded agent dialog');
    const id=mode==='shell'?'fixture-shell':'fixture-reader';
    send(id,'bounded_agent_query');
    await waitFor(f=>f.text.includes(id),'bounded profile filtered');
    send('\r','bounded_agent_select');
    const label=mode==='shell'?'Fixture-Shell':'Fixture-Reader';
    const selected=await waitFor(f=>f.text.includes(label+' ·') && !f.text.includes('Select agent'),'bounded profile selected');
    await shot('bounded-profile-selected',selected,{titlecased:selected.text.includes(label+' ·'),
      no_success_toast:!selected.text.includes('agent: '),no_requests:counts().transcript===0});
    if(mode==='variants') {
      for(let i=0;i<4;i++) {
        if(i) {
          send('\x14','bounded_ctrl_t');
          const expected=i===1?'fast':i===2?'none':null;
          const cycled=await waitFor(f=>{
            const metadata=f.cells.map(row=>row.map(c=>c.symbol).join('')).filter(row=>row.includes(label+' ·') && row.includes('OpenCode Zen')).at(-1) || '';
            return metadata && (expected ? metadata.includes(expected) : !metadata.includes(' fast') && !metadata.includes(' none'));
          },'bounded variant '+i);
          await shot('bounded-variant-cycle-'+i,cycled,{titlecased:cycled.text.includes(label+' ·'),no_success_toast:!cycled.text.includes('agent: ')});
        }
        submit();
        const done=await waitFor(f=>f.text.includes('BOUNDED-VARIANT-'+i+': completed.') &&
          counts().transcript===i+1 && counts().completed===i+2,'bounded variant request '+i);
        const request=logs.filter(e=>e.kind==='provider' && e.operation==='transcript').at(-1);
        await shot('bounded-variant-request-'+i,done,{valid_request:request.valid,
          actual_effort:request.reasoning_effort===[null,'high','low',null][i],no_extra_requests:counts().title===1 && counts().invalid===0});
      }
    } else {
      submit();
      const done=await waitFor(f=>f.text.includes('BOUNDED-SHELL:') && counts().completed===4,'real bash outputs and final answer');
      await shot('bounded-shell-completed',done,{exact_requests:counts().transcript===3 && counts().title===1 && counts().invalid===0,
        two_calls:logs.filter(e=>e.kind==='fixture_tool_call').length===2,
        verified_outputs:logs.filter(e=>e.kind==='provider' && e.operation==='transcript').at(-1).fixture_outputs_verified,
        owner_success_status:origin==='upstream'
          ? done.text.includes('Command exited with code 0.')
          : !done.text.includes('Command exited with code 0.'),
        complete_output_budget:done.text.includes(origin==='upstream'?'(36 earlier lines)':'(34 earlier lines)')});
      checks.exit_zero_text_visible=done.text.includes('Command exited with code 0');save();
      const locate=f=>visibleMatches(f,origin==='upstream'?"$ printf 'SHELL-LINE-%02d":'$ printf SHELL-LINE-%02d');
      const click=async()=>{
        const f=await frame(), hits=locate(f);
        if(hits.length!==1) throw Error('Long Shell header not unique');
        const {x,y}=hits[0];
        send(`\x1b[<0;${x+3};${y+1}M`,'bounded_shell_down');send(`\x1b[<0;${x+3};${y+1}m`,'bounded_shell_up');
      };
      const hits=locate(done);if(hits.length!==1) throw Error('Long Shell header not unique');
      send(`\x1b[<35;${hits[0].x+3};${hits[0].y+1}M`,'bounded_shell_hover');
      const hovered=await waitFor(f=>locate(f).length===1 && JSON.stringify(f.cells)!==JSON.stringify(done.cells),'Shell hover style');
      await shot('bounded-shell-hover',hovered,{hover_changed:true,no_requests:counts().transcript===3});
      await click();
      await resize(80);
      const expanded=await waitFor(f=>f.rows===80 && f.text.includes('SHELL-LINE-01') && f.text.includes('SHELL-LINE-40') && !/\(\d+ earlier lines\)/.test(f.text),'expanded real Shell output');
      await shot('bounded-shell-expanded',expanded,{last_line_visible:true,no_requests:counts().transcript===3});
      await click();
      await resize(40);
      const collapsed=await waitFor(f=>f.rows===40 && /\(\d+ earlier lines\)/.test(f.text) && !f.text.includes('SHELL-LINE-01'),'recollapsed real Shell output');
      await shot('bounded-shell-recollapsed',collapsed,{first_line_hidden:true,no_requests:counts().transcript===3});
    }
    checks.status='PASS';save();return checks;
  } catch(e) {checks.status='FAILED';checks.reason=e.message;save();throw e;}
}
