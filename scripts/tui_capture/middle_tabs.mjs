// VIS44: real packets, current painted targets and read-only durable owners.
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';

export async function probeMiddleTabs({origin,dir,send,waitFor,frame,capture,visibleMatches,logs,control,relaunch,resize,middleCase,railPersistence=false}) {
  const names=middleCase==='clipped'?['Alpha','Beta','Gamma','Delta']:['Alpha','Beta','Gamma'];
  const title=name=>`VIS44 ${name} retained middle-close history`;
  const draft=name=>`VIS44 draft ${name} Ω界`;
  const result={origin,middle_case:middleCase,status:'IN_PROGRESS',checks:[],native_manual_width:railPersistence?'Location-scoped existing deck CAS; Original has a global layout preference':'process-local; real drag, not persisted layout qualification'};
  const save=()=>fs.writeFileSync(path.join(dir,'middle-tabs-checks.json'),JSON.stringify(result,null,2)+'\n');
  const record=(stage,data={})=>{result.checks.push({stage,...data});save();};
  const poll=async(predicate,label)=>{const end=Date.now()+15000;while(Date.now()<end){if(predicate())return;await new Promise(r=>setTimeout(r,25));}throw Error('Timed out '+label);};
  const snap=async stage=>{control({kind:'middle_snapshot',request_id:stage});await poll(()=>logs.some(e=>e.kind==='middle_snapshot'&&e.request_id===stage),'owner '+stage);const s=logs.find(e=>e.kind==='middle_snapshot'&&e.request_id===stage);record(stage,{snapshot:s});return s;};
  const shot=async(stage,predicate)=>{const f=await waitFor(predicate,stage);record(stage,{capture_status:await capture('middle-tabs-'+stage,f,'CAPTURED'),cursor:f.cursor,columns:f.columns,rows:f.rows});return f;};
  const requests=()=>logs.filter(e=>e.kind==='provider');
  const paste=text=>send('\x1b[200~'+text+'\x1b[201~','vis44_paste');
  const newHome=async()=>{send('\x18','vis44_new_leader');await new Promise(r=>setTimeout(r,100));send('n','vis44_new_home');await waitFor(f=>f.text.includes('Ask anything'),'actual fresh Home');};
  const roots=s=>s.observations.filter(o=>['sessions','session_v2','session'].includes(o.table)).flatMap(o=>o.rows).filter(r=>!r.parent_id);
  const deck=s=>{
    if(origin==='oc') {
      const prefs=s.observations.filter(o=>o.table==='prefs').flatMap(o=>o.rows).filter(r=>r.key.startsWith('tui.selection.tab_deck:'));
      if(prefs.length!==1)throw Error('Native checked deck not unique');
      return JSON.parse(prefs[0].value).sessions;
    }
    const entries=s.client_persistence.filter(p=>p.path.endsWith('/tui/tabs.json'));
    if(entries.length!==1)throw Error('Original real tabs persistence not unique');
    const data=entries[0].data;
    const scopes=[data.global,...Object.values(data.cwd||{})].filter(s=>s?.tabs?.length);
    if(scopes.length>1)throw Error('Original fixture has multiple nonempty tab scopes');
    return (scopes[0]?.tabs||[]).map(t=>t.sessionID);
  };
  const journal=s=>JSON.stringify(s.observations.filter(o=>['messages','message','session_message','turns','tool_ops','tool_executions','tool_operations'].includes(o.table)));
  const acceptedRail=s=>{
    if(origin!=='oc'||!railPersistence)return;
    const prefs=s.observations.filter(o=>o.table==='prefs').flatMap(o=>o.rows).filter(r=>r.key.startsWith('tui.selection.tab_deck:'));
    if(prefs.length!==1||JSON.parse(prefs[0].value).rail_width!==5)throw Error('Accepted Location rail width was lost');
  };
  let baseline, ids, remaining;
  const unchanged=s=>{
    if(journal(s)!==journal(baseline)||JSON.stringify(s.project_files)!==JSON.stringify(baseline.project_files)||requests().length!==names.length*2||requests().some(r=>!r.valid)||logs.some(e=>e.kind==='fixture_tool_call'))throw Error('Close/reopen changed history or admitted effects');
    for(const name of names)if(!roots(s).some(r=>r.id===ids[name]&&r.title===title(name)))throw Error('Closed Root identity/history was deleted');
    if(JSON.stringify(deck(s))!==JSON.stringify(remaining))throw Error('Actual saved deck differs from Middle Down result');
  };
  const target=(f,name,optional=false)=>{
    if(middleCase==='compact') {
      const ordinal=String(remaining.indexOf(ids[name])+1);
      const hits=visibleMatches(f,ordinal).filter(p=>p.x<5);
      if(optional&&hits.length===0)return null;
      if(hits.length!==1)throw Error('Actual compact ordinal target not unique');
      return hits[0];
    }
    const hits=visibleMatches(f,'VIS44 '+name).filter(p=>middleCase==='vertical'?p.x<42:p.y===0);
    if(optional&&hits.length===0)return null;
    if(hits.length!==1)throw Error('Actual visible tab body not unique: '+name);
    return {x:hits[0].x+6,y:hits[0].y};
  };
  const close=async(name,stage,selected)=>{
    const before=await frame(),p=target(before,name);
    record(stage+'-target',{name,id:ids[name],point:p,no_prior_move_or_activation:true});
    send(`\x1b[<1;${p.x+1};${p.y+1}M`,'vis44_middle_down');
    remaining=remaining.filter(id=>id!==ids[name]);
    // Compact Original can overlay its survivor with the new hovered tab's
    // title. Capture that real state; durable Down checks precede any release.
    await shot(stage+'-down',f=>selected?f.text.includes(draft(selected)):f.text.includes('Ask anything')&&!f.text.includes('VIS44-ANSWER-'));
    const after=await snap(stage+'-owner-after-down');unchanged(after);
    if(origin==='oc') {
      const prefs=after.observations.filter(o=>o.table==='prefs').flatMap(o=>o.rows).find(r=>r.key.startsWith('tui.selection.tab_deck:'));
      if(JSON.parse(prefs.value).active!==(selected?ids[selected]:null))throw Error('Middle Down changed the survivor owner');
    }
    if(selected) {
      send(`\x1b[<35;${before.columns-1};${before.rows-2}M`,'vis44_leave_closed_tab_hover');
      await shot(stage+'-survivor',f=>f.text.includes('VIS44-ANSWER-'+selected)&&f.text.includes(draft(selected)));
    }
    const saved=JSON.stringify(after.observations.filter(o=>o.table==='prefs').flatMap(o=>o.rows).filter(r=>r.key.startsWith('tui.selection.tab_deck:')));
    send(`\x1b[<1;${p.x+1};${p.y+1}m\x1b[<33;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'vis44_inert_release_drag_left_up');
    await new Promise(r=>setTimeout(r,350));
    const released=await snap(stage+'-owner-after-release');unchanged(released);
    if(origin==='oc'&&JSON.stringify(released.observations.filter(o=>o.table==='prefs').flatMap(o=>o.rows).filter(r=>r.key.startsWith('tui.selection.tab_deck:')))!==saved)throw Error('Middle Up/Drag leaked another save/action');
  };
  try {
    await shot('home',f=>f.text.includes('Ask anything'));
    for(const [index,name] of names.entries()) {
      if(index)await newHome();
      paste('VIS44 seed '+name);await waitFor(f=>f.text.includes('VIS44 seed '+name),'painted seed '+name);send('\r','vis44_submit');
      await waitFor(f=>f.text.includes('VIS44-ANSWER-'+name)&&!f.text.includes('esc interrupt'),'actual completed '+name);
      await poll(()=>logs.filter(e=>e.kind==='provider_completed').length===(index+1)*2,'actual title/completion '+name);
      paste(draft(name));await waitFor(f=>f.text.includes(draft(name)),'independent unsent '+name);
    }
    baseline=await snap('seeded-owner');
    ids=Object.fromEntries(names.map(name=>{const found=roots(baseline).filter(r=>r.title===title(name));if(found.length!==1)throw Error('Actual durable Root not unique');return [name,found[0].id];}));
    remaining=names.map(name=>ids[name]);unchanged(baseline);
    await shot('seeded',f=>f.text.includes(draft(names.at(-1))));
    if(middleCase==='compact') {
      send('\x1b[<0;42;11M','vis44_actual_rail_drag_down');
      send('\x1b[<32;5;11M','vis44_actual_rail_drag');
      await waitFor(f=>visibleMatches(f,'1').filter(p=>p.x<5).length===1,'real compact rail');
      send('\x1b[<0;5;11m','vis44_actual_rail_drag_up');
      await resize(24,80);
      const compact=await shot('compact',f=>f.columns===80&&f.rows===24&&visibleMatches(f,'1').some(p=>p.x<5));
      record('compact-geometry',{actual_drag:true,width:5,restart_persistence_claim:railPersistence});
      if(railPersistence) {
        const ordinals=names.map((_,index)=>{
          const hits=visibleMatches(compact,String(index+1)).filter(p=>p.x<5);
          if(hits.length!==1||hits[0].x!==2||hits[0].y!==3+2*index)throw Error('Actual compact top allocation differs');
          return hits[0];
        });
        record('compact-layout',{width:5,actual_ordinals:ordinals});
        acceptedRail(await snap('accepted-rail-owner'));
      }
    }
    // Derive membership from the current painted frame, including clipped strips.
    const current=await frame();
    const inactive=names.slice(0,-1).find(name=>target(current,name,true)!==null);
    if(!inactive)throw Error('No visible inactive Root');
    await close(inactive,'inactive',names.at(-1));
    if(names.length===4) {
      const extra=names.slice(0,-1).find(name=>remaining.includes(ids[name]));
      // After the first removal the three-tab strip must expose the remaining root.
      await close(extra,'clipped-survivor',names.at(-1));
    }
    const active=names.at(-1),survivor=names.find(name=>remaining.includes(ids[name])&&name!==active);
    await close(active,'active',survivor);
    await close(survivor,'last',null);
    await shot('last-release-home',f=>f.text.includes('Ask anything')&&!f.text.includes('VIS44-ANSWER-'));
    if(middleCase==='compact') {
      // Relaunch uses the bridge's original physical size; restore both actual
      // frontends first, rather than comparing an 80-column parser to 160 PTY.
      await resize(48,160);
      await shot('pre-exit-restored-geometry',f=>f.columns===160&&f.rows===48&&f.text.includes('Ask anything'));
    }
    send('\x03','vis44_natural_exit');await poll(()=>logs.some(e=>e.kind==='exit'&&e.generation===0),'natural generation0 exit');
    const exit=logs.find(e=>e.kind==='exit'&&e.generation===0);if(exit.code!==0||exit.termination!=='natural')throw Error('Root close quit or exit was not natural');
    await relaunch();
    await shot('restart-home',f=>f.text.includes('Ask anything')&&!f.text.includes('VIS44-ANSWER-'));
    const restarted=await snap('restart-owner');unchanged(restarted);acceptedRail(restarted);
    send('/sessions','vis44_history');await waitFor(f=>f.text.includes('/sessions'),'sessions draft');send('\r','vis44_history_enter');
    await waitFor(f=>f.text.includes('Sessions'),'actual Sessions dialog');
    send('VIS44 Alpha','vis44_history_filter');
    const list=await shot('history-dialog',f=>f.text.includes('Sessions')&&f.text.includes(title('Alpha')));
    const heading=list.cells.findIndex(row=>row.map(c=>c.symbol).join('').includes('Sessions'));
    const hits=visibleMatches(list,title('Alpha')).filter(p=>p.y>heading);
    if(hits.length!==1)throw Error('Original closed history title not unique in dialog');
    const p=hits[0];send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'vis44_history_mouse_reopen');
    const reopened=await shot('history-reopened',f=>!f.text.includes('Sessions')&&f.text.includes('VIS44-ANSWER-Alpha'));
    remaining=[ids.Alpha];const restored=await snap('reopened-original-id');unchanged(restored);acceptedRail(restored);
    if(railPersistence) {
      const ordinal=visibleMatches(reopened,'1').filter(p=>p.x<5);
      if(ordinal.length!==1||ordinal[0].x!==2||ordinal[0].y!==3)throw Error('Restart did not restore the actual compact rail geometry');
      record('durable-rail',{width:5,restored_on_history_reopen:true,preference_scope:origin==='oc'?'Location':'global',native_cas_observed:origin==='oc',actual_ordinal:ordinal[0],cursor:reopened.cursor});
    }
    record('proof',{root_ids:ids,requests:requests().length,tool_calls:0,history_unchanged:true,closed_deck_not_resurrected:true,reopened_same_id:ids.Alpha});
    result.status='PASS_BEHAVIOR_ONLY';save();return result;
  } catch(error) {
    result.status='FAILED';record('failure',{reason:error.stack});
    const f=await frame();await capture('middle-tabs-failure',f,'DIAGNOSTIC_FAILED_PREDICATE');return result;
  }
}
