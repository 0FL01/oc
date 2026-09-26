// VIS33 opt-in paired actual binaries; no DB writes or fabricated UI state.
import fs from 'node:fs';
import path from 'node:path';

export async function probeRevertRedo({origin,dir,send,waitFor,frame,capture,visibleMatches,logs,control,relaunch}) {
  const result={status:'IN_PROGRESS',checks:[],origin};
  const save=()=>fs.writeFileSync(path.join(dir,'revert-redo-checks.json'),JSON.stringify(result,null,2)+'\n');
  const counts=()=>({requests:logs.filter(e=>e.kind==='provider').length,completed:logs.filter(e=>e.kind==='provider_completed').length,
    transcript:logs.filter(e=>e.kind==='provider'&&e.operation==='transcript').length,
    titles:logs.filter(e=>e.kind==='provider'&&e.operation==='title').length,
    tools:logs.filter(e=>e.kind==='fixture_tool_call').length,invalid:logs.filter(e=>e.kind==='provider'&&!e.valid).length});
  let baseline;
  const unchanged=()=>!baseline||JSON.stringify(counts())===JSON.stringify(baseline);
  const record=(stage,details={})=>{result.checks.push({stage,counts:counts(),...details});save();};
  const shot=async(stage,predicate)=>{
    const f=await waitFor(f=>predicate(f)&&unchanged(),stage,15000);
    const status=await capture('revert-redo-'+stage,f,'CAPTURED');record(stage,{capture_status:status,zero_extra_calls:unchanged()});
    if(status!=='CAPTURED')throw Error('Unstable '+stage);return f;
  };
  const unique=(f,label)=>{const m=visibleMatches(f,label);if(m.length!==1)throw Error(`Nonunique ${label}: ${JSON.stringify(m)}`);return m[0];};
  const mouse=(p,kind='click')=>send(kind==='hover'?`\x1b[<35;${p.x+1};${p.y+1}M`:
    `\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'vis33_'+kind);
  const snapshot=async(stage,visible,reverted)=>{
    control({kind:'revert_snapshot',request_id:stage});
    await waitFor(()=>logs.some(e=>e.kind==='revert_snapshot'&&e.request_id===stage),'read-only '+stage,6000);
    const observation=logs.find(e=>e.kind==='revert_snapshot'&&e.request_id===stage);
    record(stage,{observation});
    const files={project:observation.project_files,config:observation.config_files};
    if(!result.immutable_files)result.immutable_files=files;
    else if(JSON.stringify(result.immutable_files)!==JSON.stringify(files))throw Error('Fixture workspace/config mutated '+stage);
    const rows=observation.observations.flatMap(o=>o.rows);
    if(!rows.some(r=>r.archived_users===3&&r.visible_users===visible&&r.reverted_users===reverted))
      throw Error('Durable boundary/count mismatch '+stage+': '+JSON.stringify(rows));
    return rows;
  };
  const prompt=i=>`VIS33 user turn ${i}. No tools.`;
  const restored=f=>[1,2,3].every(i=>f.text.includes(`VIS33-ANSWER-${i}`))&&!f.text.includes('messages reverted');
  const reverted=f=>f.text.includes('2 messages reverted')&&f.text.includes('ctrl+x r')&&f.text.includes('/redo')&&
    f.text.includes('VIS33-ANSWER-1')&&![2,3].some(i=>f.text.includes(`VIS33-ANSWER-${i}`))&&!f.text.includes('Message Actions');
  const clear=async()=>{send('\x1b[F'+'\x7f'.repeat(160),'vis33_clear_draft');
    await waitFor(f=>!f.cells[f.cursor.y].map(c=>c.symbol).join('').includes('VIS33')&&
      !f.cells[f.cursor.y].map(c=>c.symbol).join('').includes('/redo'),'cleared restored draft',6000);};
  const revert=async(stage)=>{
    await clear();const f=await frame();const targets=visibleMatches(f,prompt(2)).filter(p=>p.y!==f.cursor.y);
    if(targets.length!==1)throw Error('Nonunique transcript turn 2');
    const p=targets[0];mouse({x:p.x+5,y:p.y});
    const popup=await shot(stage+'-popup',f=>f.text.includes('Message Actions')&&visibleMatches(f,'Revert').length===1);
    mouse(unique(popup,'Revert'));const after=await shot(stage+'-normal',reverted);
    if(!visibleMatches(after,prompt(2)).some(p=>p.y===after.cursor.y))throw Error('Selected prompt not restored');
    await snapshot(stage+'-db',1,2);return after;
  };
  try {
    await shot('home',f=>f.text.includes('Ask anything'));
    for(let i=1;i<=3;i++) {
      send('\x1b[200~'+prompt(i)+'\x1b[201~','vis33_fixture_paste');send('\r','vis33_fixture_submit');
      await shot('turn-'+i,f=>f.text.includes(`VIS33-ANSWER-${i}`)&&counts().transcript===i&&counts().completed===counts().requests&&counts().titles===1);
    }
    baseline=counts();result.baseline=baseline;save();
    if(baseline.requests!==4||baseline.invalid||baseline.tools)throw Error('Expected three completed real transcript requests and independent title');
    await snapshot('completed-db',3,0);
    const normal=await revert('card');let p=unique(normal,'2 messages reverted');mouse({x:p.x+5,y:p.y},'hover');
    const hover=await shot('card-hover',f=>reverted(f)&&JSON.stringify(f.cells[p.y])!==JSON.stringify(normal.cells[p.y]));
    // Drag selects actual terminal text; release over marker must not restore it.
    p=unique(hover,'2 messages reverted');send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<32;${p.x+12};${p.y+1}M\x1b[<0;${p.x+12};${p.y+1}m`,'vis33_selection_drag');
    await shot('selection-guard',reverted);await snapshot('selection-guard-db',1,2);
    // Dismiss selection by clicking the focused editor, then one marker click.
    mouse({x:normal.cursor.x,y:normal.cursor.y});
    mouse(unique(await frame(),'2 messages reverted'));await shot('card-restored',restored);await snapshot('card-restored-db',3,0);
    for(const mode of ['shortcut','slash','palette']) {
      await revert(mode);
      if(mode==='shortcut')send('\x18r','vis33_configured_redo_shortcut');
      else {
        await clear();
        if(mode==='slash'){send('/redo','vis33_slash');await waitFor(f=>f.text.includes('/redo'),'typed redo',5000);send('\r','vis33_slash_enter');}
        else {send('\x10','vis33_palette');await waitFor(f=>f.text.includes('Commands'),'palette opened',6000);
          send('Redo','vis33_palette_filter');const popup=await shot('palette-open',f=>f.text.includes('Redo'));
          const targets=visibleMatches(popup,'Redo');mouse(targets.at(-1));}
      }
      await shot(mode+'-restored',restored);await snapshot(mode+'-restored-db',3,0);
    }
    await revert('durable');await clear();send('/new','vis33_switch_new');
    await waitFor(f=>f.text.includes('/new'),'new command ready',6000);send('\r','vis33_switch_enter');
    await shot('switched-home',f=>f.text.includes('Ask anything')&&!f.text.includes('messages reverted'));
    const reopen=async(stage)=>{send('/sessions','vis33_sessions');
      await waitFor(f=>f.text.includes('/sessions'),'sessions command ready',6000);send('\r','vis33_sessions_enter');
      const popup=await shot(stage+'-sessions',f=>f.text.includes('Sessions')&&f.text.includes('VIS33 saved tail fixture'));
      const rows=visibleMatches(popup,'VIS33 saved tail fixture').filter(p=>p.y>0);
      if(rows.length!==1)throw Error('Nonunique session dialog row');
      mouse(rows[0]);return shot(stage,reverted);};
    await reopen('reopened');await snapshot('reopened-db',1,2);
    await clear();send('\x03','vis33_clean_quit');
    await waitFor(()=>logs.some(e=>e.kind==='exit'&&e.generation===0),'clean exit',15000);
    await relaunch();await waitFor(f=>f.text.includes('MiMo'),'restart prompt',15000);
    if(reverted(await frame()))await shot('restarted',reverted);else await reopen('restarted');
    await snapshot('restarted-db',1,2);
    // Exercise real pager inputs across durable history; count remains owner-owned.
    send('\x1b[5~','vis33_page_up');await shot('pager-up',f=>f.text.includes('2 messages reverted'));
    send('\x1b[6~','vis33_page_down');await shot('pager-down',reverted);await snapshot('pager-db',1,2);
    mouse(unique(await frame(),'2 messages reverted'));await shot('restart-card-restored',restored);await snapshot('final-db',3,0);
    await clear();send('\x03','vis33_final_clean_quit');
    await waitFor(()=>logs.some(e=>e.kind==='exit'&&e.generation===1),'final clean exit',15000);
    const exits=logs.filter(e=>e.kind==='exit');record('clean-exits',{exits});
    if(exits.length!==2||exits.some(e=>e.code!==0||e.termination!=='natural'))throw Error('Unclean fixture exit');
    result.status='PASS';
  } catch(error) {
    result.status='FAILED';result.reason=error.message;record('failure',{reason:error.message});
    await capture('revert-redo-failure',await frame(),'FAILED_STATE');
  }
  save();return result;
}
