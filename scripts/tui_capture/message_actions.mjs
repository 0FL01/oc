// Opt-in actual PTY probe. Every target comes from that side's styled VT grid.
import fs from 'node:fs';
import path from 'node:path';

export async function probeMessageActions({origin,dir,send,waitFor,frame,capture,visibleMatches,logs,chunks,prompt,forkRevert=false}) {
  const result={status:'IN_PROGRESS',checks:[],native_conversation:{status:'NOT_RUN'}};
  const counts=()=>({requests:logs.filter(e=>e.kind==='provider').length,
    completed:logs.filter(e=>e.kind==='provider_completed').length,
    invalid:logs.filter(e=>e.kind==='provider'&&!e.valid).length});
  const save=()=>fs.writeFileSync(path.join(dir,'message-actions-checks.json'),JSON.stringify(result,null,2)+'\n');
  const record=(stage,details)=>{result.checks.push({stage,provider_counts:counts(),...details});save();};
  const unique=(f,label)=>{const m=visibleMatches(f,label);if(m.length!==1)throw Error(`Expected unique ${label}: ${JSON.stringify(m)}`);return m[0];};
  const mouse=(p,kind)=>{
    const bytes=kind==='hover'?`\x1b[<35;${p.x+1};${p.y+1}M`:
      `\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`;
    record(kind,{point:p,bytes_base64:Buffer.from(bytes).toString('base64')});send(bytes,'message_actions_'+kind);
  };
  const baseline=counts();result.baseline=baseline;
  const unchanged=()=>JSON.stringify(counts())===JSON.stringify(baseline);
  try {
    if(baseline.requests!==3 || baseline.completed!==3 || baseline.invalid!==0)throw Error('Incomplete real read/title baseline');
    const before=await frame(), p=unique(before,prompt);
    mouse({x:p.x+4,y:p.y},'hover');
    const hovered=await waitFor(f=>JSON.stringify(f.cells[p.y])!==JSON.stringify(before.cells[p.y])&&unchanged(),'message user hover',6000);
    await capture('message-actions-user-hover',hovered,'CAPTURED');
    mouse({x:unique(hovered,prompt).x+4,y:p.y},'click');
    const popup=await waitFor(f=>f.text.includes('Message Actions')&&['Jump to','Revert','Copy','Fork'].every(s=>visibleMatches(f,s).length===1)&&unchanged(),'Message Actions popup',6000);
    await capture('message-actions-popup',popup,'CAPTURED');
    record('popup',{options:['Jump to','Revert','Copy','Fork'].map(label=>({label,...unique(popup,label)})),provider_unchanged:unchanged()});
    const copy=unique(popup,'Copy');mouse(copy,'hover');
    const copyHover=await waitFor(f=>f.text.includes('Message Actions')&&
      JSON.stringify(f.cells[copy.y])!==JSON.stringify(popup.cells[copy.y])&&unchanged(),'Copy hover',6000);
    await capture('message-actions-copy-hover',copyHover,'CAPTURED');
    const offset=Buffer.concat(chunks[0]).length;
    mouse(unique(copyHover,'Copy'),'click');
    let after;
    try {after=await waitFor(f=>!f.text.includes('Message Actions')&&unchanged(),'real Copy result',6000);}
    catch(e){after=await frame();record('copy-failed',{reason:e.message,visible_text:after.text});}
    await capture('message-actions-copy-after',after,'CAPTURED');
    const vt=Buffer.concat(chunks[0]).subarray(offset).toString('latin1');
    const payloads=[...vt.matchAll(/\x1b\]52;[^;]*;([^\x07\x1b]*)(?:\x07|\x1b\\)/g)].map(m=>Buffer.from(m[1],'base64').toString());
    const predicates={dialog_closed:!after.text.includes('Message Actions'),
      osc52_exact_prompt:payloads.includes(prompt),provider_unchanged:unchanged()};
    record('copy-effect',{predicates,copy_feedback:after.text.includes('Copied to clipboard'),osc52_payloads:payloads,system_clipboard:'NOT_VERIFIED_BY_PTY'});
    result.status=Object.values(predicates).every(Boolean)?'PASS':'FAILED_COPY_EFFECT';
  } catch(e) {result.status='FAILED';record('failed',{reason:e.message});await capture('message-actions-failure',await frame(),'FAILED_STATE');}
  // The approved native contract is evaluated independently, including after a
  // real failed Copy. Redo restores the whole tail under the c452180 amendment.
  if(origin==='oc') {
    try {
      if((await frame()).text.includes('Message Actions')){send('\x1b','message_actions_dismiss');await waitFor(f=>!f.text.includes('Message Actions'),'dismiss popup',6000);}
      const second='Second same-session spacing check?';
      send('\x1b[200~'+second+'\x1b[201~','native_second_paste');send('\r','native_second_submit');
      await waitFor(f=>f.text.includes('GEOMETRY-TURN-TWO')&&counts().requests===5&&counts().completed===5,'native second completed turn',12000);
      const nativeBaseline=counts();result.native_conversation.baseline=nativeBaseline;
      const noCalls=()=>JSON.stringify(counts())===JSON.stringify(nativeBaseline);
      const command=async(name,draft='')=>{
        if(draft)send('\x7f'.repeat([...draft].length),'native_clear_restored_draft');
        send(name,'native_conversation_command');
        await waitFor(f=>f.text.includes(name),'native typed '+name,6000);
        send('\x1b','native_dismiss_suggestions');
        await waitFor(f=>f.text.includes(name)&&!f.text.includes('undo the last')&&!f.text.includes('redo the last'),'native dismissed suggestions',6000);
        send('\r','native_execute_'+name.slice(1));
      };
      const stages=[['undo-one','/undo','',true,false,second],['undo-two','/undo',second,false,false,prompt],
        ['redo-all','/redo',prompt,true,true,'']];
      for(const [stage,name,draft,first,secondVisible,restored] of stages) {
        await command(name,draft);
        const f=await waitFor(f=>f.text.includes('GEOMETRY-SHORT')===first && f.text.includes('GEOMETRY-TURN-TWO')===secondVisible &&
          (!restored || visibleMatches(f,restored).some(p=>p.y===f.cursor.y)) && noCalls(),'native '+stage,8000);
        await capture('native-conversation-'+stage,f,'CAPTURED_NATIVE_APPROVED_DIVERGENCE');
        record(stage,{predicates:{first_answer_visible:first,second_answer_visible:secondVisible,restored_draft:restored||null,zero_extra_provider_calls:noCalls()},cursor:f.cursor});
      }
      result.native_conversation.status='PASS';
      if(forkRevert) {
        result.native_fork_revert={status:'IN_PROGRESS',baseline:nativeBaseline};save();
        const rootBefore=(await frame()).cells[0].map(c=>c.symbol).join('');
        const open=async(text)=>{
          const p=unique(await frame(),text);mouse({x:p.x+4,y:p.y},'click');
          return waitFor(f=>f.text.includes('Message Actions')&&noCalls(),'native action popup',6000);
        };
        let popup=await open(second);
        await capture('native-conversation-source-revert-popup',popup,'CAPTURED_NATIVE_APPROVED_DIVERGENCE');
        mouse(unique(popup,'Revert'),'click');
        const sourceReverted=await waitFor(f=>!f.text.includes('Message Actions')&&f.text.includes('GEOMETRY-SHORT')&&
          !f.text.includes('GEOMETRY-TURN-TWO')&&visibleMatches(f,second).some(p=>p.y===f.cursor.y)&&noCalls(),'native source Revert effect',8000);
        await capture('native-conversation-source-revert-after',sourceReverted,'CAPTURED_NATIVE_APPROVED_DIVERGENCE');
        record('source-revert-effect',{predicates:{dialog_closed:true,first_answer_retained:true,selected_answer_absent:true,
          selected_prompt_restored:true,zero_extra_provider_calls:noCalls()},cursor:sourceReverted.cursor});
        await command('/redo',second);
        await waitFor(f=>f.text.includes('GEOMETRY-SHORT')&&f.text.includes('GEOMETRY-TURN-TWO')&&f.cursor.x===5&&noCalls(),'restore source before fork',8000);
        popup=await open(second);
        await capture('native-conversation-fork-popup',popup,'CAPTURED_NATIVE_APPROVED_DIVERGENCE');
        mouse(unique(popup,'Fork'),'click');
        const forked=await waitFor(f=>!f.text.includes('Message Actions')&&f.text.includes('GEOMETRY-SHORT')&&
          !f.text.includes('GEOMETRY-TURN-TWO')&&visibleMatches(f,second).some(p=>p.y===f.cursor.y)&&
          f.cells[0].map(c=>c.symbol).join('')!==rootBefore&&!f.text.includes('Subagents')&&noCalls(),'native new fork root and draft',8000);
        await capture('native-conversation-fork-root',forked,'CAPTURED_NATIVE_APPROVED_DIVERGENCE');
        record('fork-root',{predicates:{new_tab_header:forked.cells[0].map(c=>c.symbol).join('')!==rootBefore,
          first_answer_retained:true,selected_turn_absent:true,selected_prompt_restored:true,
          no_subagent_chrome:!forked.text.includes('Subagents'),zero_extra_provider_calls:noCalls()},cursor:forked.cursor});
        send('\x7f'.repeat([...second].length),'native_clear_fork_draft');
        await waitFor(f=>!f.text.includes(second)&&f.cursor.x===5,'empty fork draft',6000);
        popup=await open(prompt);
        await capture('native-conversation-revert-popup',popup,'CAPTURED_NATIVE_APPROVED_DIVERGENCE');
        mouse(unique(popup,'Revert'),'click');
        const reverted=await waitFor(f=>!f.text.includes('Message Actions')&&!f.text.includes('GEOMETRY-SHORT')&&
          !f.text.includes('GEOMETRY-TURN-TWO')&&visibleMatches(f,prompt).some(p=>p.y===f.cursor.y)&&noCalls(),'native Revert hides fork turn and restores prompt',8000);
        await capture('native-conversation-revert-after',reverted,'CAPTURED_NATIVE_APPROVED_DIVERGENCE');
        record('revert-effect',{predicates:{dialog_closed:true,answers_absent:true,prompt_restored:true,zero_extra_provider_calls:noCalls()},cursor:reverted.cursor});
        result.native_fork_revert.status='PASS';
      }
    }catch(e){if(result.native_fork_revert){result.native_fork_revert.status='FAILED';result.native_fork_revert.reason=e.message;}
      else {result.native_conversation.status='FAILED';result.native_conversation.reason=e.message;}
      result.status='FAILED';await capture('native-conversation-failure',await frame(),'FAILED_STATE');}
  }
  save();return result;
}
