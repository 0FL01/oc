// VIS16/17 real tool cards and read-only owner facts; not a renderer substitute.
import fs from 'node:fs';
import path from 'node:path';
import {probeHomeMcp} from './home_mcp.mjs';

export async function probeToolPreview({origin,dir,send,waitFor,frame,capture,visibleMatches,logs,control,relaunch,resize,cursorProbe,mcpProbe,promptProbe,historyProbe,userShellProbe}) {
  const result={origin,status:'IN_PROGRESS',stages:[],differences:mcpProbe?[
    'Native intrinsic Connected bold follows frozen source contract; the running pinned original may lose this attribute.',
    'Native repeated keyboard/mouse investigation must populate an unsent draft; pinned Home may not reinject an identical route prompt after the user clears it. This reference difference is observed, not native acceptance.',
    'Native read-only details and unsent investigation use the safe owner diagnostic, not a raw peer exception; full styled-frame parity remains independently open.'
  ]:[
    'Native operation body/streams stay at existing 2048-byte bounds; expansion never opens cold output.',
    'Typed native guidance is separate from body; donor tool-output guidance may be displayed as recorded body.',
    'Compact native viewing/capture status has no promised donor pixel equivalent.'
  ]};
  const save=()=>fs.writeFileSync(path.join(dir,'tool-preview-checks.json'),JSON.stringify(result,null,2)+'\n');
  const requests=()=>logs.filter(e=>e.kind==='provider').length;
  const calls=()=>logs.filter(e=>e.kind==='fixture_tool_call').length;
  let draftCaret=null;
  const poll=async(predicate,label)=>{const end=Date.now()+6000;while(Date.now()<end){if(predicate())return;await new Promise(r=>setTimeout(r,25));}throw Error('Timed out '+label);};
  const snapshot=async stage=>{control({kind:'tool_preview_snapshot',request_id:stage});await poll(()=>logs.some(e=>e.kind==='tool_preview_snapshot'&&e.request_id===stage),stage);return logs.find(e=>e.kind==='tool_preview_snapshot'&&e.request_id===stage);};
  const shot=async(stage,predicate,checks={})=>{
    const f=await waitFor(predicate,stage,15000);
    if(draftCaret)checks={...checks,draft_preserved:f.text.includes('preserved draft'),
      prompt_caret_preserved:JSON.stringify(f.cursor)===JSON.stringify(draftCaret)};
    const status=await capture('tool-preview-'+stage,f,'CAPTURED_TOOL_PREVIEW');
    result.stages.push({stage,status,cursor:f.cursor,requests:requests(),calls:calls(),checks});save();
    if(status!=='CAPTURED_TOOL_PREVIEW'||Object.values(checks).some(v=>v===false))throw Error('Bad capture/check '+stage);
    return f;
  };
  const slash=async text=>{send(text,'tool_preview_command');await waitFor(f=>f.text.includes(text),'draft '+text,6000);send('\r','tool_preview_command_enter');};
  const target=(f,index)=>visibleMatches(f,(origin==='oc'?'vis16__output':'vis16_output')+' [index='+index);
  const click=async index=>{
    const hits=target(await frame(),index);if(hits.length!==1)throw Error('Generic header not unique '+index);
    const p=hits[0];send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'tool_preview_click');
  };
  const reopen=async stage=>{
    await slash('/sessions');
    const f=await waitFor(f=>f.text.includes('Sessions')&&f.text.includes('VIS16 tool preview fixture'),'saved preview session',6000);
    const hits=visibleMatches(f,'VIS16 tool preview fixture').filter(p=>p.y>0);if(hits.length!==1)throw Error('Session title not unique');
    const p=hits[0];send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'tool_preview_reopen');
    return shot(stage,f=>f.text.includes('VIS16-DONE:')&&target(f,1).length===1&&!f.text.includes('output: VIS-MCP-FIRST'));
  };
  try {
    if(userShellProbe) {
      result.differences=['Native direct-user Shell uses the existing bounded supervised Jobs owner, NULL model turn and data-only command/completion facts. Original uses session.shell. No model generation is a substitute. Full styled grids/PNGs/cursors remain unmasked; full parts/mode/Mini restoration is not claimed.'];
      result.user_shell=await userShellProbe({origin,send,shot,waitFor,frame,logs,snapshot,relaunch});
      result.status='PASS_BEHAVIOR_ONLY';save();return result;
    }
    if(historyProbe) {
      result.differences=['Native can browse from an unfinished draft and Down restores it, per frozen VIS12. The pinned original may refuse arbitrary nonempty text at index zero; that real frame is observed before explicit reference-only clear/browse, and its forward boundary is empty. Full styled grid/PNG/cursor differences remain unmasked.'];
      result.prompt_history=await historyProbe({origin,send,shot,waitFor,frame,visibleMatches,resize,logs,snapshot,relaunch});
      result.status='PASS_BEHAVIOR_ONLY';save();return result;
    }
    if(promptProbe) {
      result.differences=['Caret behavior is proved by real edits and one exact provider input/durable native user row; full styled-grid/PNG/cursor differences remain unmasked.'];
      result.prompt_caret=await promptProbe({origin,send,shot,waitFor,frame,visibleMatches,resize,logs,snapshot,relaunch});
      result.status='PASS_BEHAVIOR_ONLY';save();return result;
    }
    if(mcpProbe) {
      result.mcp_status=await mcpProbe({origin,send,shot,waitFor,frame,visibleMatches,resize,logs,snapshot});
      result.status='PASS_BEHAVIOR_ONLY';save();return result;
    }
    await shot('home',f=>f.text.includes('Ask anything')&&f.text.includes('1 MCP /mcps'));
    if(!cursorProbe)result.home_mcp=await probeHomeMcp({send,shot,waitFor,frame,visibleMatches,resize,logs,control,snapshot});
    send('\x1b[200~VIS16 preview: execute the supplied MCP and Shell calls exactly once.\x1b[201~','tool_preview_prompt');send('\r','tool_preview_submit');
    const done=await shot('completed',f=>f.text.includes('VIS16-DONE:')&&(cursorProbe||target(f,1).length===1));
    if(requests()!==6||calls()!==4||logs.some(e=>e.kind==='provider'&&!e.valid))throw Error('Unexpected provider/call count or unpaired result');
     const before=await snapshot('completed');result.before=before;
     if(before.mcp_calls.length!==3)throw Error('Real MCP did not execute exactly three times');
     if(before.shell_effect?.lines!==1||before.shell_effect?.bytes!==17)throw Error('Real Shell did not produce exactly one bounded effect');
    if(origin==='oc') {
      const data=before.observations.flatMap(o=>o.data.presentation||[]);
      if(data.length!==4||data.some(p=>Buffer.byteLength(p.presentation.body)>2048))throw Error('Missing/unbounded structured body');
      if(data.filter(p=>p.presentation.generated_guidance).length<3||data.some(p=>p.presentation.body.includes('[tool output:')))throw Error('Body/guidance not separated');
      if(done.text.includes('[tool output:')||done.text.includes('parts omitted from bounded history preview'))throw Error('Generated guidance/notice in dialogue');
      result.owner_facts={bounded:true,separate_body_guidance:true,presentation_count:data.length,artifacts:Object.keys(before.artifacts).length};
    }
    // An expanded first card exceeds the base viewport. Use the same tall
    // profile on both binaries rather than asserting offscreen text is absent.
    await resize(80);
    await waitFor(f=>f.rows===80&&target(f,1).length===1,'tall card view',6000);
    send('\x1b[200~preserved draft\x1b[201~','tool_preview_draft');
    const base=await waitFor(f=>f.text.includes('preserved draft'),'draft');
     if(cursorProbe) {
       result.cursor_temporal=await cursorProbe();
       const after=await snapshot('cursor_completed');result.after=after;
       if(requests()!==6||calls()!==4||JSON.stringify(before.artifacts)!==JSON.stringify(after.artifacts)||JSON.stringify(before.mcp_calls)!==JSON.stringify(after.mcp_calls)||JSON.stringify(before.observations)!==JSON.stringify(after.observations)||JSON.stringify(before.shell_effect)!==JSON.stringify(after.shell_effect))throw Error('Cursor interactions changed effects/captures/RAW');
       result.cursor_temporal.no_tool_replay=true;
       result.status='DIAGNOSTIC_CURSOR_ONLY';save();return result;
    }
    draftCaret=base.cursor;
    const p=target(base,1)[0];
    send(`\x1b[<35;${p.x+1};${p.y+1}M`,'tool_preview_hover');
    await shot('hover',f=>JSON.stringify(f.cells)!==JSON.stringify(base.cells)&&!f.text.includes('output: VIS-MCP-FIRST'));
    for(const index of [1,2]) {
      await click(index);
      const label=index===1?'FIRST':'SECOND';
      const expanded=await shot('mcp-'+index+'-expanded',f=>f.text.includes('output: VIS-MCP-'+label+'-BEGIN')&&f.text.includes('available parameter')&&f.text.includes('[Part preview truncated]'));
      if(!['[Part preview truncated]','[output preview truncated; full result retained]','[truncated]'].every(s=>expanded.text.includes(s))||expanded.text.includes('DISTANT-END'))throw Error('Lost literal payload or recovered discarded bytes');
      if(origin==='oc'&&expanded.text.includes('[tool output:'))throw Error('Generated model guidance displayed as body');
      await click(index);
      await shot('mcp-'+index+'-recollapsed',f=>target(f,index).length===1&&!f.text.includes('output: VIS-MCP-'+label));
    }
    const errorText=origin==='oc'?'mcp tool reported failure':'VIS-MCP-ERROR';
    await click(4);
    await shot('error-expanded',f=>f.text.includes(errorText));
    await click(4);
    await shot('error-recollapsed',f=>target(f,4).length===1&&!f.text.includes(errorText));
    const shellTarget=f=>visibleMatches(f,'$ printf').filter(p=>p.y>0);
    const shellClick=async()=>{const hits=shellTarget(await frame());if(hits.length!==1)throw Error('Shell header not unique');const p=hits[0];send(`\x1b[<0;${p.x+1};${p.y+1}M\x1b[<0;${p.x+1};${p.y+1}m`,'tool_preview_shell_click');};
    await shellClick();
    await resize(80);
    await shot('shell-expanded',f=>f.rows===80&&f.text.includes('VIS-SHELL-LINE-080')&&!/\(\d+ earlier lines\)/.test(f.text));
    await shellClick();await resize(40);
    // Geometry changed deliberately; the same draft stays at the resized prompt.
    draftCaret=null;
    await shot('shell-recollapsed',f=>f.rows===40&&/\(\d+ earlier lines\)/.test(f.text));
    send('\x03','tool_preview_clear_draft');await waitFor(f=>!f.text.includes('preserved draft'),'draft cleared',6000);
    if(origin==='oc') {
      await resize(20);await waitFor(f=>f.rows===20,'short resource-details viewport',6000);
      await slash('/cards');
      await waitFor(f=>f.text.includes('Tool cards')&&f.text.includes('vis16__output completed'),'native cards list',6000);
      // Newest is the failure; select the oldest successful MCP operation.
      send('\x1b[B\x1b[B\x1b[B\r','tool_preview_cards_detail');
      const detail=await waitFor(f=>f.text.includes('Tool result')&&f.text.includes('Capture Complete:')&&f.text.includes('Body preview:')&&f.text.includes('Available output page (bounded; explicit viewer read):'),'native bounded capture details',6000);
      if(detail.cursor.visible)throw Error('Read-only detail exposes underlying composer caret');
      await capture('native-tool-preview-capture-details',detail,'NATIVE_ONLY_RESOURCE_DETAILS');
      send('\x1b[6~','tool_preview_cards_page_down');
      await waitFor(f=>f.text.includes('Tool result')&&f.text.includes('enter next page'),'bounded cards next-page availability',6000);
      send('\r','tool_preview_cards_next_page');
      const paged=await waitFor(f=>f.text.includes('Tool result')&&f.text.includes('bytes 240–480 of'),'existing explicit bounded owner page',6000);
      if(paged.cursor.visible)throw Error('Read-only paging exposes underlying composer caret');
      await capture('native-tool-preview-cards-paged',paged,'NATIVE_ONLY_RESOURCE_DETAILS');
      send('\x1b','tool_preview_close_card_detail');await waitFor(f=>f.text.includes('Tool cards')&&!f.text.includes('Tool result'),'cards list restored',6000);
      send('\x1b','tool_preview_close_cards');await waitFor(f=>!f.text.includes('Tool cards')&&!f.text.includes('Tool result'),'cards closed',6000);
      await resize(40);await waitFor(f=>f.rows===40,'restore paired geometry',6000);
    }
    await slash('/new');await shot('switched',f=>f.text.includes('Ask anything'));
    await reopen('reopened');
    const after=await snapshot('reopened');
     if(requests()!==6||calls()!==4||JSON.stringify(before.artifacts)!==JSON.stringify(after.artifacts)||JSON.stringify(before.mcp_calls)!==JSON.stringify(after.mcp_calls)||JSON.stringify(before.observations)!==JSON.stringify(after.observations)||JSON.stringify(before.shell_effect)!==JSON.stringify(after.shell_effect))throw Error('View interactions changed effects/captures/RAW');
    send('\x03','tool_preview_exit');await poll(()=>logs.some(e=>e.kind==='exit'&&e.generation===0),'natural exit');
    await relaunch();await waitFor(f=>f.text.includes('Ask anything')||f.text.includes('VIS16-DONE:'),'restart ready',15000);
    // Native restores the deck; donor may return Home. Reopen through the actual list on both sides.
    await reopen('restarted');
    const restarted=await snapshot('restarted');result.after=restarted;
     if(requests()!==6||calls()!==4||JSON.stringify(before.artifacts)!==JSON.stringify(restarted.artifacts)||JSON.stringify(before.mcp_calls)!==JSON.stringify(restarted.mcp_calls)||JSON.stringify(before.observations)!==JSON.stringify(restarted.observations)||JSON.stringify(before.shell_effect)!==JSON.stringify(restarted.shell_effect))throw Error('Restart replayed effects or changed captures/RAW');
    result.status='PASS_BEHAVIOR_ONLY';
  } catch(error) {
    result.status='FAILED';result.reason=error.message;
    const failed=await frame();result.failure_cursor=failed.cursor;
    await capture('tool-preview-failure',failed,'FAILED_STATE');
  }
  save();return result;
}
