// VIS16/VIS31: actual VT commands and raster blink, never final-only.
import fs from 'node:fs';
import path from 'node:path';

export async function probeCursorTemporal({origin,dir,page,send,frame,waitFor,capture,recordTemporal,visibleMatches,mode,cardCase,geometry,resize}) {
  const result={origin,mode,card_case:cardCase,geometry,status:'IN_PROGRESS',states:[]};
  const save=()=>fs.writeFileSync(path.join(dir,'cursor-temporal-checks.json'),JSON.stringify(result,null,2)+'\n');
  const draft='preserved draft Ω界';
  send('\x03','cursor_clear_draft');
  await waitFor(f=>!f.text.includes('preserved draft'),'clear temporal draft',6000);
  send('\x1b[200~'+draft+'\x1b[201~\x1b[D\x1b[D','cursor_unicode_draft');
  await waitFor(f=>f.text.includes(draft),'Unicode draft inside caret',15000);
  await resize(geometry.rows);
  await waitFor(f=>f.rows===geometry.rows&&f.text.includes(draft),'representative cursor geometry',6000);
  const header=f=>visibleMatches(f,(origin==='oc'?'vis16__output':'vis16_output')+' [index=1');
  const cardView=async()=>{
    for(let i=0;i<=16;i++) {
      const f=await frame();
      if(header(f).length===1)return f;
      if(i===16)throw Error('Temporal card header not reachable');
      send('\x1b[<64;12;5M','cursor_scroll_card_into_view');
      await new Promise(r=>setTimeout(r,100));
    }
  };
  let base=await cardView();
  if(cardCase==='expanded') {
    const p=header(base)[0];
    send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'cursor_expand_real_card');
    await waitFor(f=>JSON.stringify(f.cells)!==JSON.stringify(base.cells),'actual card expansion',6000);
    base=await cardView();
    if(!base.text.includes('available parameter')||!base.text.includes('VIS-MCP-FIRST-BEGIN'))throw Error('Expanded card parameters/body not visible');
  }
  const p=header(base)[0], composer=base.cursor;
  let expected=composer;
  const states=['idle','hover','restored','search-idle','search-hover','composer-restored'];
  for(const state of states) {
    const search=state.startsWith('search');
    if(state==='search-idle') {
      send('\x10','cursor_open_command_search');
      const f=await waitFor(f=>f.text.includes('Commands')&&f.cursor.visible&&f.cursor.y!==composer.y,'focused search owns caret',6000);
      expected=f.cursor;
    } else if(state==='composer-restored') {
      send('\x1b','cursor_close_command_search');
      await waitFor(f=>!f.text.includes('Commands')&&f.text.includes(draft)&&JSON.stringify(f.cursor)===JSON.stringify(composer),'exact composer draft/caret restored',6000);
      expected=composer;
    }
    await page.evaluate(({state,expected})=>startCursorState(state,expected),{state,expected});
    const start=performance.now();let moves=0;
    const timer=state.includes('hover')?setInterval(()=>{
        const location=moves++%3;
        const x=search?geometry.columns/2:p.x+1;
        const y=search?Math.min(geometry.rows-3,expected.y+3+(location===1?2:0)):Math.min(geometry.rows-8,p.y+1+(location===1?5:0));
        send(`\x1b[<35;${location===2?1:x};${location===2?1:y}M`,'cursor_continuous_hover');
      },20):null;
    try {
      while(performance.now()-start<5600) {
        await new Promise(r=>setTimeout(r,20));
      }
    } finally { if(timer)clearInterval(timer); }
    const {snapshots,...trace}=await page.evaluate(()=>finishCursorState());
    const shots=snapshots.length;
    recordTemporal(state,snapshots);
    fs.writeFileSync(path.join(dir,'cursor-'+state+'.trace.json'),JSON.stringify(trace,null,2)+'\n');
    const f=await frame(), phases=trace.samples.map(s=>s.raster.visible),rises=[];
    for(let i=1;i<phases.length;i++)if(!phases[i-1]&&phases[i])rises.push(trace.samples[i].at_ms);
    const periods=rises.slice(1).map((time,i)=>time-rises[i]);
    const gaps=trace.samples.slice(1).map((s,i)=>s.at_ms-trace.samples[i].at_ms);
    const phantom=trace.commands.filter(c=>c.cursor.visible&&!c.cursor.synchronized&&(c.cursor.x!==expected.x||c.cursor.y!==expected.y));
    const item={state,moves,temporal_full_frames:shots,samples:trace.samples.length,commands:trace.commands.length,cycles:periods.length,
      phantom_command_states:phantom.length,draft_visible:f.text.includes(draft),
      final_caret_preserved:JSON.stringify(f.cursor)===JSON.stringify(expected),
      cycle_periods_ms:periods,max_sample_gap_ms:Math.max(0,...gaps),raster_blink_observed:periods.length>=3};
    result.states.push(item);save();
    await capture('cursor-'+state,f,'CAPTURED_CURSOR_DIAGNOSTIC');
  }
  // Cadence is relative to this owner's measured idle profile, not an invented clock.
  for(const item of result.states) {
    const baseline=result.states.find(s=>s.state===(item.state.startsWith('search')?'search-idle':'idle'));
    const idle=baseline.cycle_periods_ms;
    const error=baseline.max_sample_gap_ms+item.max_sample_gap_ms;
    item.adequate_raster_sampling=mode!=='blink'||(idle.length>=3&&item.max_sample_gap_ms<=Math.min(...idle)/4);
    item.cadence_preserved=mode!=='blink'||(item.adequate_raster_sampling&&item.cycle_periods_ms.length>=3&&item.cycle_periods_ms.every(p=>p>=Math.min(...idle)-error&&p<=Math.max(...idle)+error));
  }
  result.status=result.states.every(s=>s.phantom_command_states===0&&s.final_caret_preserved&&s.cadence_preserved&&(!s.state.includes('hover')||s.commands>0)&&
    (mode==='blink'?s.raster_blink_observed:s.cycles===0))?'QUALIFIED_CURSOR_BEHAVIOR_ONLY':'FAILED_CURSOR_QUALIFICATION';
  save();return result;
}
