// Opt-in VIS30: live owner-created sessions, actual PTY actions, read-only DB facts.
import fs from 'node:fs';
import path from 'node:path';

export async function probeSessions({origin,dir,send,waitFor,frame,capture,visibleMatches,logs,control,relaunch,priorEvidence,selectedScope}) {
  const result={status:'IN_PROGRESS',checks:[],multi_day:'INCOMPLETE: no genuine archived public record supplied; timestamps never seeded'};
  const save=()=>fs.writeFileSync(path.join(dir,'sessions-checks.json'),JSON.stringify(result,null,2)+'\n');
  const counts=()=>({requests:logs.filter(e=>e.kind==='provider').length,
    completed:logs.filter(e=>e.kind==='provider_completed').length,
    invalid:logs.filter(e=>e.kind==='provider'&&!e.valid).length});
  let baseline;
  const unchanged=()=>!baseline || JSON.stringify(counts())===JSON.stringify(baseline);
  const snapshot=async stage=>{
    control({kind:'sessions_snapshot',request_id:stage});
    const deadline=Date.now()+5000;
    while(!logs.some(e=>e.kind==='sessions_snapshot'&&e.request_id===stage)&&Date.now()<deadline)
      await new Promise(r=>setTimeout(r,50));
    const observation=logs.find(e=>e.kind==='sessions_snapshot'&&e.request_id===stage);
    if(!observation)throw Error('Read-only snapshot timed out: '+stage);
    const rows=observation.observations.flatMap(o=>o.rows||[]);
    result.checks.push({stage,provider_counts:counts(),observation});save();
    return rows;
  };
  const shot=async(stage,predicate)=>{
    const f=await waitFor(f=>predicate(f)&&unchanged(),stage,15000);
    const status=await capture('sessions-'+stage,f,'CAPTURED');
    result.checks.push({stage,provider_counts:counts(),provider_unchanged:unchanged(),capture_status:status});save();
    if(status!=='CAPTURED')throw Error('Unstable '+stage);
    return f;
  };
  let query='';
  const open=async stage=>{
    query='';
    send('/sessions','sessions_open_type');await waitFor(f=>f.text.includes('/sessions'),'typed /sessions',5000);
    send('\r','sessions_open_enter');return shot(stage,f=>f.text.includes('Sessions')&&f.text.includes('ctrl+d'));
  };
  const modal=f=>f.text.includes('Sessions')&&f.text.includes('ctrl+d');
  const esc=async stage=>{send('\x1b','sessions_escape');return shot(stage,f=>!modal(f));};
  const clear=()=>{send('\x1b[F'+'\x7f'.repeat([...query].length),'sessions_clear_query');query='';};
  const search=async(title,stage)=>{
    clear();send(title,'sessions_search');query=title;
    return shot(stage,f=>f.text.includes('Sessions')&&visibleMatches(f,title).length>=2);
  };
  const create=async(subject,marker,stage)=>{
    const before=counts();
    const prompt=`Discuss ${subject} in one short sentence. Include ${marker} exactly. Do not use tools.`;
    send('\x1b[200~'+prompt+'\x1b[201~','sessions_fixture_prompt');send('\r','sessions_fixture_submit');
    const f=await waitFor(f=>f.text.includes(marker)&&counts().completed>=before.completed+2&&
      logs.some(e=>e.kind==='provider_completed'&&e.operation==='title'&&e.generated_title?.trim()&&
        e.live_request_index>before.requests&&visibleMatches(f,e.generated_title.trim().slice(0,18)).length>0),stage,110000);
    await capture('sessions-'+stage,f,'CAPTURED');
    const rows=await snapshot(stage+'-db');
    const titles=logs.filter(e=>e.kind==='provider_completed'&&e.operation==='title').map(e=>e.generated_title.trim());
    const row=rows.find(r=>r.title&&titles.includes(r.title)&&!result.fixtures?.some(old=>old.id===r.id));
    if(!row)throw Error('Generated title not observed in owner DB');
    result.fixtures ??= [];result.fixtures.push({...row,marker,prompt});save();return row;
  };
  try {
    const empty=await snapshot('initial-owned-root');
    const resume=logs.find(e=>e.kind==='launch')?.sessions_resume;
    if(!resume && empty.some(r=>r.title))throw Error('Fixture root not empty');
    result.preexisting_fixture_rows=empty;save();
    const prior=priorEvidence?JSON.parse(fs.readFileSync(priorEvidence,'utf8')):null;
    const retained=resume && empty.find(r=>r.title&&r.directory?.endsWith('/other-project')&&
      (r.title.includes('VIS30-LUNAR') || prior?.fixtures.some(old=>old.id===r.id&&old.directory?.endsWith('/other-project'))));
    const foreign=retained || await create('lunar geology','VIS30-LUNAR','other-root-generated');
    if(retained){result.fixtures=[retained];await capture('sessions-other-root-retained',await frame(),'CAPTURED_RETAINED_FIXTURE');}
    send('\x03','sessions_other_quit');
    await waitFor(()=>logs.some(e=>e.kind==='exit'&&e.generation===0),'other cwd clean exit',15000);
    await relaunch();
    await waitFor(f=>f.text.includes('Ask anything'),'current cwd home',15000);
    const first=prior?empty.find(r=>r.id===prior.fixtures.find(r=>r.marker==='VIS30-OCEAN')?.id):
      await create('ocean currents','VIS30-OCEAN','first-root-generated');
    if(!first)throw Error('Prior actual ocean root unavailable');
    // Supported new-session command; never pre-seed title or history rows.
    let second;
    if(prior) {
      second=selectedScope==='cwd'?empty.find(r=>r.id===prior.fixtures.find(r=>r.marker==='VIS30-FOREST')?.id):foreign;
      if(!second)throw Error('Selected retained fixture unavailable');
      result.fixtures=[foreign,first,second];result.prior_evidence=priorEvidence;
      result.selected_row='retained fixture-owned '+selectedScope+' root; actual generated title provenance in prior capture';
    } else {
      send('/new','sessions_new_type');await waitFor(f=>f.text.includes('/new'),'typed /new',5000);send('\r','sessions_new_enter');
      await waitFor(f=>f.text.includes('Ask anything'),'new root home',10000);
      second=await create('forest ecology','VIS30-FOREST','second-root-generated');
    }
    baseline=counts();result.baseline=baseline;save();
    const expected=prior?(retained?0:2):retained?4:6;
    if(baseline.requests!==expected || baseline.completed!==expected || baseline.invalid)throw Error('Expected real transcript/title pairs');
    let all=await open('all-open');
    if(!all.text.includes(foreign.title)) {send('\x01','sessions_scope_all');all=await shot('all-scope',f=>f.text.includes(foreign.title));}
    for(const row of [foreign,first,second])if(!all.text.includes(row.title))throw Error('All scope missing actual root '+row.id);
    await snapshot('all-before-actions');
    send('\x01','sessions_scope_cwd');
    await shot('cwd-scope',f=>f.text.includes(first.title)&&!f.text.includes(foreign.title)&&
      (prior || f.text.includes(second.title)));
    await search(first.title,'search-first');
    send('\r','sessions_select_enter');
    await shot('entered-first',f=>!modal(f)&&f.text.includes('VIS30-OCEAN'));
    await open('reopen-first');await esc('escape-first');await open('reopen-after-escape');
    const selected=selectedScope!=='cwd'?foreign:second;
    result.selected_fixture=selected;save();
    if(selectedScope!=='cwd'){send('\x01','sessions_select_foreign_all');await shot('selected-all-scope',f=>f.text.includes(foreign.title)&&f.text.includes('current directory'));}
    // Rename the non-current selected row and verify the exact owner ID changed.
    await search(selected.title,'selected-second');send('\x12','sessions_selected_rename');
    await shot('rename-dialog',f=>f.text.includes('Rename')&&f.text.includes(selected.title));
    send('\x1b[F'+'\x7f'.repeat([...selected.title].length),'sessions_clear_rename_title');
    const renamed='VIS30 Renamed lunar geology';send(renamed,'sessions_rename_input');
    await shot('rename-input',f=>f.text.includes(renamed)&&f.text.includes('Rename'));
    send('\r','sessions_rename_commit');
    await shot('rename-return',f=>!f.text.includes('Rename session'));
    const afterRename=await snapshot('renamed-db');
    if(afterRename.find(r=>r.id===selected.id)?.title!==renamed || afterRename.find(r=>r.id===first.id)?.title!==first.title)
      throw Error('Selected-row rename owner effect mismatch');
    const current=await frame();if(!modal(current))await open('rename-reopened');
    await search(renamed,'renamed-search');
    if(selectedScope!=='cwd') {
      send('\r','sessions_enter_selected_root');
      try {
      await shot('entered-selected-root',f=>!modal(f)&&f.text.includes('VIS30-LUNAR'));
      await snapshot('entered-selected-db');
      await open('selected-root-reopen');
      send('\x01','sessions_selected_root_cwd');
      await shot('selected-root-cwd',f=>f.text.includes(renamed)&&!f.text.includes(first.title)&&
        f.text.includes('all projects')&&f.text.includes('Sessions for other-project'));
      send('\x01','sessions_selected_root_all');
      await shot('selected-root-all',f=>f.text.includes(renamed)&&f.text.includes(first.title)&&f.text.includes('current directory'));
      await search(first.title,'return-current-search');send('\r','sessions_return_current_enter');
      await shot('returned-current',f=>!modal(f)&&f.text.includes('VIS30-OCEAN')&&!f.text.includes('VIS30-LUNAR'));
      await open('return-current-reopen');await search(renamed,'delete-selected-search');
      } catch(error) {
        // Preserve the strict entry predicate and failed result while qualifying
        // an independent delete. Never treat the error frame as successful entry.
        result.failed_stages ??= [];result.failed_stages.push({stage:'selected-root-entry-and-return',reason:error.message});save();
        await capture('sessions-selected-entry-failure',await frame(),'FAILED_STATE');
        if(!modal(await frame()))throw error;
        await snapshot('selected-entry-failure-db');
        await search(renamed,'delete-selected-search-after-failure');
      }
    }
    send('\x04','sessions_delete_arm');
    await shot('delete-confirm',f=>f.text.includes('Press ctrl+d again to confirm'));
    const armed=await snapshot('armed-db');if(!armed.some(r=>r.id===selected.id))throw Error('Delete happened before confirmation');
    send('\x04','sessions_delete_confirm');
    await shot('delete-after',f=>!f.text.includes('Press ctrl+d again to confirm')&&f.text.includes('No sessions'));
    const deleted=await snapshot('deleted-db');
    if(deleted.some(r=>r.id===selected.id)||!deleted.some(r=>r.id===first.id)||
      (second.id!==selected.id&&!deleted.some(r=>r.id===second.id)))throw Error('Delete owner effect mismatch');
    const deletedFromAll=selectedScope!=='cwd';
    clear();await shot(deletedFromAll?'all-after-delete':'cwd-after-delete',f=>f.text.includes(first.title)&&!f.text.includes(renamed));
    send('\x01','sessions_scope_after_delete');
    await shot(deletedFromAll?'cwd-after-delete':'all-after-delete',f=>f.text.includes(first.title)&&
      (deletedFromAll || f.text.includes(foreign.title))&&!f.text.includes(renamed));
    await esc('final-escape');
    if(!unchanged())throw Error('Dialog actions changed provider counts');
    result.status=result.failed_stages?.length?'FAILED':'PASS';
    result.scope=result.failed_stages?.length?'RENAME_DELETE_PASS; SELECTED_ROOT_ENTRY_FAILED; multi-day incomplete':'CURRENT_DAY_FUNCTIONAL; multi-day incomplete';
  } catch(error) {
    result.status='FAILED';result.reason=error.message;
    await capture('sessions-failure',await frame(),'FAILED_STATE');await snapshot('failure-db');
  }
  save();return result;
}
