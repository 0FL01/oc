// Real two-child lifecycle and observed component phases, never a UI story.
const ROOT='BACKGROUND_SUBAGENT_LIFECYCLE';
const TITLE='Background subagent fixture';
const descriptions={A:'Сравнение subagent с upstream',B:'Состояние Git и тесты'};
const glyphs=row=>row.map(cell=>cell.width===0?'':cell.symbol).join('');
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const parse=value=>typeof value==='string'?JSON.parse(value):value;
const assert=(condition,message)=>{if(!condition)throw Error(message);};
export const backgroundCanonical=value=>JSON.stringify(value,(_,entry)=>entry&&typeof entry==='object'&&!Array.isArray(entry)?Object.fromEntries(Object.entries(entry).sort(([a],[b])=>a.localeCompare(b))):entry);

export function backgroundLaunch(launch,origin) {
  const home=launch?.env.HOME;
  assert(home&&launch.env.TMPDIR===home+'/tmp'&&launch.env.XDG_DATA_HOME===home+'/data'
    &&launch.argv.length===2&&launch.argv[1]===(origin==='upstream'?'--standalone':'tui'),
    'Actual private application launch and owned HOME/data/TMPDIR are required');
  return launch.env.TMPDIR;
}

export function backgroundFacts(snapshot) {
  const {background:data,issued}=snapshot.composer;
  assert(data&&['native','ts'].includes(data.schema)&&data.root,'Recognized owned schema required');
  const children={},notices=[];
  if(data.schema==='native') {
    for(const key of ['A','B']) {
      const ops=data.operations.filter(op=>op.provider_call_id===issued[key].call_id);
      assert(ops.length===1&&ops[0].name==='subagent','Exactly one admitted native call '+key);
      const op=ops[0],jobs=data.children.filter(job=>job.operation_id===op.id);
      assert(jobs.length===1,'Exact native launch owner '+key);
      const job=jobs[0],identity=parse(job.identity);
      assert(job.parent_id===data.root&&job.relational_parent===data.root&&identity.parent===data.root&&identity.child===job.child_id&&identity.operation===op.id&&identity.delivery_id===job.delivery_id&&identity.background===true,'Native full launch fence '+key);
      assert(backgroundCanonical(parse(op.input))===backgroundCanonical(issued[key].arguments),'Native admitted arguments '+key);
      children[key]={child:job.child_id,operation:op.id,delivery:job.delivery_id,generation:identity.generation,location:identity.location,state:job.state};
      if(job.message_id) {
        const message=data.messages.find(message=>message.id===job.message_id);
        assert(message&&data.notices.some(event=>event.payload===job.delivery_id),'Native durable notice receipt '+key);
        notices.push({key,id:job.message_id,seq:message.seq,location:'message'});
      }
    }
  } else {
    const parts=data.messages.filter(message=>message.type==='assistant').flatMap(message=>parse(message.data).content||[]);
    for(const key of ['A','B']) {
      const tools=parts.filter(part=>part.type==='tool'&&part.id===issued[key].call_id);
      assert(tools.length===1&&tools[0].name==='subagent','Exact current TS tool '+key);
      const tool=tools[0],child=tool.state.metadata?.sessionID;
      assert(child&&data.children.filter(row=>row.id===child&&row.parent_id===data.root).length===1,'TS relational child owner '+key);
      assert(tool.state.status==='completed'&&tool.state.metadata.status==='running','Returned TS background provenance '+key);
      assert(backgroundCanonical(tool.state.input)===backgroundCanonical(issued[key].arguments),'TS admitted arguments '+key);
      children[key]={child,operation:tool.id,item:tool.providerState?.itemId};
    }
    const messages=data.messages.filter(message=>message.type==='synthetic').map(message=>({...message,value:parse(message.data),location:'message'}));
    const pending=data.inbox.filter(message=>message.type==='synthetic').map(message=>({...message,seq:message.enqueued_seq,value:parse(message.payload),location:'inbox'}));
    for(const message of [...messages,...pending])if(message.value.metadata?.source==='subagent') {
      const key=Object.keys(children).find(key=>children[key].child===message.value.metadata.childID);
      assert(key&&message.value.metadata.state==='completed','TS typed terminal notice owner');
      assert(message.value.description===issued[key].arguments.description,'TS typed terminal notice description');
      notices.push({key,id:message.id,seq:message.seq,location:message.location});
    }
  }
  assert(Object.keys(children).length===2&&children.A.child!==children.B.child,'Independent child identities');
  assert(notices.length===new Set(notices.map(notice=>notice.id)).size&&notices.length===new Set(notices.map(notice=>notice.key)).size,'No duplicate completion notice');
  const assignments=(data.assignments||[]).map(event=>parse(event.payload));
  assert(new Set(assignments.map(event=>event.message)).size===assignments.length,'One native assignment per exact report');
  for(const event of assignments)assert(notices.some(notice=>notice.id===event.message)&&data.turns.some(turn=>turn.id===event.turn),'Assignment belongs to an actual parent execution and report');
  return {root:data.root,children,notices:notices.sort((a,b)=>a.seq-b.seq),idle:data.root_idle===true,assignments};
}

export function backgroundComponent(frame,key) {
  const caption='Explore Subagent — '+descriptions[key];
  const matches=frame.cells.flatMap((row,y)=>glyphs(row).includes(caption)?[{x:glyphs(row).indexOf('Explore Subagent'),y}]:[]);
  assert(matches.length===1,'Original '+key+' launch card must be unique and visible');
  const {x,y}=matches[0],rows=[];
  for(let at=y;at<Math.min(frame.rows,y+4);at++) {
    const row=frame.cells[at];rows.push(row);
    if(glyphs(row).includes(' Background '))return {x,y,glyph:frame.cells[y][x-2].symbol,rows};
    assert(at===y||!glyphs(row).includes('Subagent —'),'Background badge lost at card boundary');
  }
  throw Error('Whole Background badge missing from original '+key+' card');
}

export async function finishBackgroundChildren({backgroundCase,command,observe,logs,shot,settle}) {
  let admitted;
  const sameBoundary=['terminal','wrap'].includes(backgroundCase);
  if(sameBoundary||backgroundCase==='consumed') {
    assert(logs.some(event=>event.kind==='background_parent_flushed'),'Actual parent delta flushed before child completion');
    assert(!logs.some(event=>event.kind==='provider_completed'&&event.operation==='parent'&&event.step===2),'Parent terminal response remained held');
    if(sameBoundary) {
      await command('release-A');
      admitted=await observe('both-admitted',facts=>facts.notices.length===2);
      assert(admitted.facts.notices.map(notice=>notice.key).join(',')==='B,A','Both child notices admitted in order at the held request boundary');
      assert(!logs.some(event=>event.kind==='provider'&&event.operation==='parent'&&event.step>2),'No later parent request before both child notices were admitted');
    }
    await command('release-parent');
    if(backgroundCase==='consumed')await shot('background-parent-next',f=>f.text.includes('PARENT-NEXT'),{},true);
  }
  if(!sameBoundary)await command('release-A');
  const finished=await observe('both-completed',facts=>facts.notices.length===2&&facts.notices.every(notice=>notice.location==='message'));
  return {...(settle?await settle(['A','B']):finished),admitted};
}

export async function probeBackgroundSubagents({origin,backgroundAnimation,backgroundCase,send,shot,waitFor,frame,logs,snapshot,control,slash,relaunch,visibleMatches}) {
  const launch=logs.find(event=>event.kind==='launch');
  const isolatedTmpdir=backgroundLaunch(launch,origin);
  let serial=0;
  const command=async action=>{
    const request_id='background-control-'+(++serial);
    control({kind:'tool_preview_control',request_id,action});
    const deadline=Date.now()+6000;
    while(Date.now()<deadline&&!logs.some(event=>event.kind==='tool_preview_control_ack'&&event.request_id===request_id))await sleep(20);
    const ack=logs.filter(event=>event.kind==='tool_preview_control_ack'&&event.request_id===request_id);
    const key=action.slice('release-'.length);
    assert(ack.length===1&&ack[0].released===key&&ack[0].arrival&&(key==='parent'?ack[0].call_id===null:ack[0].call_id==='call_background_'+key&&ack[0].arrival.call_id===ack[0].call_id),'Unique payload-qualified release acknowledgement');
  };
  const observe=async(label,predicate)=>{
    const deadline=Date.now()+15000;
    while(Date.now()<deadline){const snap=await snapshot('background-'+label+'-'+(++serial)),facts=backgroundFacts(snap);if(predicate(facts))return {snapshot:snap,facts};await sleep(40);}
    throw Error('Missing actual owned background state '+label);
  };
  const settle=async keys=>observe('parent-settled-'+keys.join(''),facts=>facts.idle&&keys.every(key=>{
    if(!facts.notices.some(notice=>notice.key===key&&notice.location==='message'))return false;
    const requests=logs.filter(event=>event.kind==='provider'&&event.valid&&event.operation==='parent'&&event.step>2
      &&event.consumed_notices.some(notice=>notice.child===facts.children[key].child));
    return requests.some(request=>logs.some(event=>event.kind==='provider_completed'&&event.request===request.request))
      &&(origin==='upstream'||facts.assignments.some(assignment=>assignment.message===facts.notices.find(notice=>notice.key===key)?.id));
  }));
  const scene=(f,done)=>{
    try {
      for(const key of ['A','B']) {const component=backgroundComponent(f,key);if(done.includes(key)!==(component.glyph==='✓'))return false;}
      return done.every(key=>f.text.includes('↳ Explore finished · '+descriptions[key]));
    } catch {return false;}
  };
  const phase=(f,key,done)=>scene(f,done)&&backgroundComponent(f,key).glyph===(backgroundAnimation?'⠋':'⋯');
  send('\x1b[200~'+ROOT+'\x1b[201~\r','background_parent_submit');
  await shot('background-parent-step',f=>f.text.includes('PARENT-STEP')&&logs.filter(event=>event.kind==='provider'&&event.valid&&['A','B'].includes(event.operation)).length===2,{},true);
  assert(logs.filter(event=>event.kind==='provider'&&event.valid&&['A','B'].includes(event.operation)).length===2,'Both owned child requests actually dispatched');
  const running=await observe('running',facts=>facts.notices.length===0);
  if(backgroundCase==='lifecycle')await observe('original-parent-idle',facts=>facts.idle);
  send('\x1b[200~preserved draft\x1b[201~','background_preserved_draft');
  // The original frontend applies bracketed paste asynchronously. Observe its
  // actual insertion before moving the caret; arrival in one PTY write is not
  // an application receipt, especially when static spinners match immediately.
  const draftReady=await shot('background-draft-ready',f=>f.text.includes('preserved draft'),{},true);
  send('\x1b[D\x1b[D','background_draft_caret');
  const cadence=[];
  if(backgroundAnimation) {
    const until=Date.now()+650;
    while(Date.now()<until){const f=await frame();cadence.push({at_ms:performance.now(),A:backgroundComponent(f,'A').glyph,B:backgroundComponent(f,'B').glyph});await sleep(20);}
    for(const key of ['A','B'])assert(new Set(cadence.map(sample=>sample[key])).size>=3,'Natural unpaused '+key+' glyph progression');
  }
  const a=await shot('background-running-A',f=>phase(f,'A',[])&&f.text.includes('preserved draft')
    &&f.cursor.x===draftReady.cursor.x-2&&f.cursor.y===draftReady.cursor.y,{},true);
  await shot('background-running-B',f=>phase(f,'B',[]),{},true);
  const caret=a.cursor;
  await command('release-B');
  const mixed=await observe('B-completed',facts=>facts.notices.length===1&&facts.notices[0].key==='B');
  assert(mixed.facts.children.A.child===running.facts.children.A.child&&mixed.facts.children.B.child===running.facts.children.B.child,'Launch identities stable after B completion');
  if(backgroundCase==='lifecycle')await settle(['B']);
  await shot('background-B-complete-A-running',f=>phase(f,'A',['B']),{},true);
  const finished=await finishBackgroundChildren({backgroundCase,command,observe,logs,shot,settle});
  assert(finished.facts.notices.map(notice=>notice.key).join(',')==='B,A','Durable completion delivery order B then A');
  const complete=await shot('background-completed',f=>scene(f,['A','B'])&&f.text.includes('preserved draft'),{},true);
  assert(JSON.stringify(caret)===JSON.stringify(complete.cursor),'Parent draft caret preserved through child notices');
  const completeText=complete.cells.map(glyphs);
  const noticeTones=Object.fromEntries(['A','B'].map(key=>{
    const hit=visibleMatches(complete,'↳ Explore finished · '+descriptions[key])[0];
    return [key,complete.cells[hit.y][hit.x].fg];
  }));
  const before=completeText.findIndex(text=>text.includes('PARENT-STEP'));
  const noticeB=completeText.findIndex(text=>text.includes('↳ Explore finished · '+descriptions.B));
  assert(before>=0&&noticeB>before,'Terminal/pending notice follows the current parent step');
  if(backgroundCase==='consumed') {
    const next=completeText.findIndex(text=>text.includes('PARENT-NEXT'));
    assert(next>noticeB,'Consumed notice precedes the following step, not the whole-turn tail');
    assert(logs.some(event=>event.kind==='provider'&&event.operation==='parent'&&event.step===3&&event.consumed_notices.some(notice=>notice.child===finished.facts.children.B.child)),'Actual next parent request consumed exact B completion');
  }
  // Both the original launch and the independent completion row must open the
  // same exact child. No arrival-order selection or navigation by prose ID.
  for(const key of ['A','B'])for(const kind of ['card','notice']) {
    const f=await frame();
    const needle=kind==='card'?'Explore Subagent — '+descriptions[key]:'↳ Explore finished · '+descriptions[key];
    const hits=visibleMatches(f,needle);assert(hits.length===1,'Unique actual '+kind+' target '+key);
    const p=hits[0];send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'background_open_'+kind+'_'+key);
    await shot('background-'+kind+'-'+key+'-child',f=>f.text.includes('CHILD-'+key+'-DONE')&&!f.text.includes('preserved draft'),{},true);
    if(origin==='upstream') {
      // A child keeps the modal composer open, so session-level bindings do
      // not apply there. The actual family tab selects its root session.
      const child=await frame(),tabs=visibleMatches(child,TITLE).filter(hit=>hit.y===0);
      assert(tabs.length===1,'Exact parent family tab');
      send(`\x1b[<0;${tabs[0].x+1};1M\x1b[<0;${tabs[0].x+1};1m`,'background_return_parent_tab');
    } else send('\x1b','background_return_parent');
    // Let the actual navigation receipt settle before sending another escape
    // sequence: concatenated raw Esc + SGR can be parsed as one Alt-key input.
    await waitFor(f=>scene(f,['A','B'])&&f.text.includes('preserved draft'),'actual parent route after '+kind+' '+key,6000);
    send('\x1b[<35;1;1M','background_leave_link');
    const restored=await shot('background-'+kind+'-'+key+'-parent',f=>scene(f,['A','B'])&&f.text.includes('preserved draft'),{},true);
    assert(JSON.stringify(restored.cursor)===JSON.stringify(caret),'Captured parent caret restored after '+kind+' '+key);
  }
  const after=await observe('navigation',facts=>facts.notices.length===2);
  assert(JSON.stringify(after.facts)===JSON.stringify(finished.facts),'Navigation did not retarget/replay delivery');
  assert(Object.keys(running.snapshot.composer.project_files).length>0&&backgroundCanonical(running.snapshot.composer.project_files)===backgroundCanonical(after.snapshot.composer.project_files),'Background execution and navigation did not mutate the controlled project');
  assert(logs.every(event=>event.kind!=='provider'||event.valid),'Every actual provider request belongs to the finite contract');
  send('\x03','background_clear_draft');
  await slash('/sessions');
  const list=await waitFor(f=>f.text.includes('Sessions')&&f.text.includes(TITLE),'actual root in sessions',6000);
  const hits=visibleMatches(list,TITLE).filter(p=>p.y>0);assert(hits.length===1,'Exact saved parent session');
  send(`\x1b[<0;${hits[0].x+1};${hits[0].y+1}M\x1b[<0;${hits[0].x+1};${hits[0].y+1}m`,'background_reopen_parent');
  await waitFor(f=>scene(f,['A','B']),'actual reopened parent route',6000);
  send('\x1b[<35;1;1M','background_reopened_leave_link');
  await shot('background-reopened',f=>scene(f,['A','B'])&&['A','B'].every(key=>{
    const hit=visibleMatches(f,'↳ Explore finished · '+descriptions[key])[0];
    return hit&&f.cells[hit.y][hit.x].fg===noticeTones[key];
  }),{},true);
  return {status:'PASS_BEHAVIOR_ONLY',case:backgroundCase,animation:backgroundAnimation,cadence,isolated_application_tmpdir:isolatedTmpdir,project_files_unchanged:true,
    running,mixed,finished,after,parent_continuations:logs.filter(event=>event.kind==='provider'&&event.operation==='parent'&&event.step>2),
    component_phases:'Independent A/B observations; full unmasked frames retained, not whole-scene equality'};
}
