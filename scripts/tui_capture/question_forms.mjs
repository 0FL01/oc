// Real owned question edits and source records, not a form renderer substitute.
const PROMPT='VIS37 paired question consumer', CANCEL='VIS37 dismiss the next question';
const TITLE='VIS question fixture', DONE='VIS37-DONE: owned answers accepted.', CUSTOM='custom Ω界';
const answers=[['Rust','Python'],[CUSTOM]];
const eq=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
export async function probeQuestionForms({origin,send,shot,waitFor,frame,visibleMatches,logs,snapshot,relaunch}) {
  const providers=()=>logs.filter(e=>e.kind==='provider');
  const mains=()=>providers().filter(e=>e.operation==='question_main');
  const exit=async generation=>{
    send('\x03','question_clean_exit');
    const deadline=Date.now()+6000;
    while(!logs.some(e=>e.kind==='exit'&&e.generation===generation)) {
      if(Date.now()>deadline)throw Error('Question process did not exit');
      await new Promise(resolve=>setTimeout(resolve,25));
    }
    const event=logs.find(e=>e.kind==='exit'&&e.generation===generation);
    if(event.code!==0||event.termination!=='natural')throw Error('Question process did not restore naturally');
    return {generation,code:event.code,termination:event.termination};
  };
  const operations=s=>s.observations.flatMap(o=>o.data.question_records?.operations||[]);
  const verify=(s,cancelled=false)=>{
    const records=operations(s);
    if(records.length!==(cancelled?2:1)||s.mcp_calls.length||s.mcp_lifecycle.length||s.shell_effect!==null||Object.keys(s.artifacts).length)throw Error('Extra/missing question ownership or effects');
    if(origin==='oc') {
      const completed=records.filter(r=>r.state==='completed');
      const result=completed.length===1&&JSON.parse(completed[0].output);
      if(!eq(result?.answers,answers)||result.questions.length!==2||completed[0].provider_call_id!=='call_vis37_answer')throw Error('Native durable typed answers missing');
      if(cancelled&&records.filter(r=>r.provider_call_id==='call_vis37_cancel'&&r.state==='cancelled'&&r.output==='error: question dismissed').length!==1)throw Error('Native dismissal was not a durable cancellation');
    } else {
      const completed=records.filter(r=>r.data.state.status==='completed');
      if(completed.length!==1||!eq(completed[0].data.state.metadata?.answers,answers)||completed[0].data.id!=='call_vis37_answer')throw Error('Donor durable typed metadata missing');
      if(cancelled&&records.filter(r=>r.data.id==='call_vis37_cancel'&&r.data.state.status==='error').length!==1)throw Error('Donor dismissal did not retain its error source');
    }
  };
  const slash=async text=>{send(text,'question_command');await waitFor(f=>f.text.includes(text),'question command draft',6000);send('\r','question_command_enter');};
  const click=async (needle,tag)=>{
    const f=await waitFor(f=>visibleMatches(f,needle).length===1,'unique painted question control',6000);
    const p=visibleMatches(f,needle)[0];
    send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,tag);
  };
  const reopen=async stage=>{
    await slash('/sessions');
    const f=await waitFor(f=>f.text.includes('Sessions')&&f.text.includes(TITLE),'question saved session',6000);
    const hits=visibleMatches(f,TITLE).filter(p=>p.y>0);if(hits.length!==1)throw Error('Question saved row not unique');
    const p=hits[0];send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'question_reopen');
    return shot(stage,f=>f.text.includes(DONE)&&!f.text.includes('Sessions'));
  };
  await shot('question-home',f=>f.text.includes('Ask anything'));
  send(`\x1b[200~${PROMPT}\x1b[201~`,'question_user_prompt');send('\r','question_user_submit');
  const pending=await shot('question-pending',f=>f.text.includes('Pick languages?')&&f.text.includes('Type your own answer'));
  await click('1. [ ] Rust','question_select_rust_mouse');
  const one=await shot('question-one-selected',f=>f.text.includes('1. [✓] Rust'));
  const rust=visibleMatches(one,'1. [✓] Rust');
  if(rust.length!==1)throw Error('Painted multiselect focus owner not unique');
  const active=one.cells[rust[0].y][rust[0].x];
  // Hidden terminal position is the last emitted diff cell, not an editor
  // caret. Observe the actual focus paint before toggling; a Down+Space burst
  // can legitimately form one donor diff and two native diffs. Keep both
  // complete frames and their unmodified cursor/VT facts, not a cursor anchor.
  send('\x1b[B','question_python_focus');
  await shot('question-multiple-focused',f=>{
    const hits=visibleMatches(f,'2. [ ] Python');
    return hits.length===1&&f.cells[hits[0].y][hits[0].x].fg===active.fg&&
      f.cells[hits[0].y][hits[0].x].bg===active.bg&&f.text.includes('1. [✓] Rust');
  });
  send(' ','question_select_python');await shot('question-multiple-selected',f=>f.text.includes('1. [✓] Rust')&&f.text.includes('2. [✓] Python'));
  await click('Platform and execution…','question_next_field_mouse');await shot('question-long-field',f=>f.text.includes('Where Ω界?'));
  await click('Type your own answer','question_custom_select_mouse');await shot('question-custom-empty',f=>f.text.includes('Type your own answer'));
  send('\t','question_empty_custom_tab');
  await waitFor(f=>f.text.includes('Rust, Python')&&!f.text.includes('Where Ω界?'),'optional empty custom Tab does not add required validation',6000);
  send('\x1b[Z','question_empty_custom_backtab');
  await waitFor(f=>f.text.includes('Where Ω界?'),'empty custom field restored without reply',6000);
  await click('Type your own answer','question_empty_custom_reedit_mouse');
  send(`\x1b[200~${CUSTOM}\x1b[201~`,'question_custom_paste');await shot('question-custom-edited',f=>f.text.includes(CUSTOM));
  if(mains().length!==1)throw Error('Question edits generated provider continuation');
  send('\t','question_custom_tab_commit');
  await waitFor(f=>f.text.includes('Rust, Python')&&f.text.includes(CUSTOM),'custom Tab commits into review without reply',6000);
  send('\x1b[Z','question_custom_backtab');
  await waitFor(f=>f.text.includes('Where Ω界?')&&f.text.includes(CUSTOM),'Shift+Tab restores owned custom answer',6000);
  send('\r','question_custom_reopen_edit');
  await waitFor(f=>f.cursor.visible&&f.text.includes(CUSTOM)&&f.text.includes('esc close'),'custom answer editing after Tab/Shift+Tab',6000);
  if(mains().length!==1)throw Error('Tab/Shift+Tab/edit implicitly submitted answers');
  send('\r','question_commit_custom');await shot('question-review',f=>f.text.includes(CUSTOM)&&f.text.includes('Rust')&&f.text.includes('Python'));
  if(mains().length!==1)throw Error('Review implicitly submitted answers');
  send('\r','question_explicit_reply');await shot('question-completed',f=>f.text.includes(DONE)&&!f.text.includes('interrupt'));
  await waitFor(()=>providers().length===3,'question main/result and title',15000);
  if(mains().length!==2||providers().some(e=>!e.valid))throw Error('Question result was not one exact owned continuation');
  const completed=await snapshot('question_completed');verify(completed);
  // Scroll the actual transcript to its answered heading if it is above the
  // short profile's viewport; never fabricate/crop the completed card.
  let card=await frame();
  for(let n=0;n<8&&!card.text.includes('# Questions');n++) {
    send(`\x1b[<64;4;4M`,'question_actual_scroll_up');
    card=await waitFor(f=>f.text.includes(DONE)||f.text.includes('# Questions'),'question scroll settled',6000);
  }
  await shot('question-answered-card',f=>f.text.includes('# Questions'));
  for(let n=0;n<8&&!(await frame()).text.includes(DONE);n++) {
    send('\x1b[<65;4;4M','question_actual_scroll_down');
    await waitFor(f=>f.text.includes('# Questions')||f.text.includes(DONE),'question bottom scroll settled',6000);
  }
  await waitFor(f=>f.text.includes(DONE),'question bottom',6000);
  await slash('/new');await shot('question-switched',f=>f.text.includes('Ask anything'));
  await reopen('question-reopened');const reopened=await snapshot('question_reopened');verify(reopened);
  if(!eq(completed.observations,reopened.observations)||providers().length!==3)throw Error('Question viewing changed records/replayed');
  const firstExit=await exit(0);
  await relaunch();await waitFor(f=>f.text.includes('Ask anything')||f.text.includes(DONE),'question same-root restart',15000);
  await reopen('question-restarted');const restarted=await snapshot('question_restarted');verify(restarted);
  if(!eq(completed.observations,restarted.observations)||providers().length!==3)throw Error('Question restart replayed or changed durable source');
  send(`\x1b[200~${CANCEL}\x1b[201~`,'question_cancel_prompt');send('\r','question_cancel_submit');
  await shot('question-cancel-pending',f=>f.text.includes('Pick languages?')&&f.text.includes('Type your own answer'));
  send('\x1b','question_owned_dismiss');await shot('question-dismissed',f=>!f.text.includes('Type your own answer')&&!f.text.includes('esc interrupt'));
  const dismissed=await snapshot('question_dismissed');verify(dismissed,true);
  if(providers().length!==4||mains().length!==3||logs.filter(e=>e.kind==='fixture_tool_call').length!==2)throw Error('Dismissal answered/replayed a question');
  await slash('/new');await shot('question-cancel-switched',f=>f.text.includes('Ask anything'));
  await reopen('question-cancel-reopened');const final=await snapshot('question_cancel_reopened');verify(final,true);
  if(!eq(dismissed.observations,final.observations)||providers().length!==4||providers().some(e=>!e.valid))throw Error('Dismissed question reappeared/replayed or changed RAW');
  const finalExit=await exit(1);
  if(providers().length!==4||logs.filter(e=>e.kind==='fixture_tool_call').length!==2)throw Error('Question final shutdown replayed work');
  return {status:'OBSERVED_OWNED_QUESTION',provider_requests:4,main_requests:3,explicit_user_submissions:2,
    explicit_answer_submissions:1,question_calls:2,answers,completed,reopened,restarted,dismissed,final,
    exits:[firstExit,finalExit],
    no_edit_submission:true,no_restart_replay:true,cancellation_not_answered:true,
    tab_and_backtab_committed_without_submit:true,return_after_tab_reopens_custom:true,
    optional_empty_custom_tab_did_not_submit:true,multiselect_focus_painted_before_toggle:true,
    pending_cursor:pending.cursor,readonly_native_and_donor_source_records:true};
}
