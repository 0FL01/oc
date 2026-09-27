// Thin VIS07 real-binary scenario, using the existing leader-pending PTY runner.
import fs from 'node:fs';
import path from 'node:path';
export const pasteNavigationPrefix='VIS07 visual UpDown draft αβ caret-middle preserving every word with a boundary actual separator ';
export const pasteNavigationText='VIS11-PASTE-0\nVIS11-PASTE-1\nVIS11-PASTE-2';
export const pasteNavigationDraft=pasteNavigationPrefix+pasteNavigationText;
export const expectedPasteDraft=(origin,suffixSpace=false)=>suffixSpace
  ?pasteNavigationPrefix+pasteNavigationText+(origin==='upstream'?'  ':' ')+pasteNavigationText+(origin==='upstream'?' ':'')
  :pasteNavigationDraft+(origin==='upstream'?' ':'');

export async function probePasteNavigation({origin,dir,send,frame,capture,waitFor,visibleMatches,logs,config,suffixSpace=false}) {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  const result={origin,config,status:'IN_PROGRESS',suffix_space:suffixSpace,
    typed_raw_draft:expectedPasteDraft('oc',suffixSpace),expected_raw_draft:expectedPasteDraft(origin,suffixSpace),
    donor_difference:'Original pasteText inserts a raw trailing spacer outside the chip extmark; native retains only original pasted input',observations:[]};
  const save=()=>fs.writeFileSync(path.join(dir,'leader-checks.json'),JSON.stringify(result,null,2)+'\n');
  const shot=async name=>{await sleep(120);const f=await frame();const status=await capture('leader-'+name,f,'CAPTURED');result.observations.push({name,status,cursor:f.cursor,text:f.text,provider_requests:logs.filter(e=>e.kind==='provider').length});save();return f;};
  const paste=(text,name)=>send('\x1b[200~'+text+'\x1b[201~',name);
  try {
    send('\x14','paste_nav_variant_cycle');await sleep(200);
    paste(pasteNavigationPrefix,'paste_nav_prefix');await sleep(200);
    paste(pasteNavigationText,'paste_nav_first');
    const settleStart=performance.now();
    const settled=await waitFor(f=>visibleMatches(f,'[Pasted').length===1,'cold navigation paste chip',5000);
    result.chip_settlement={contract:'Actual chip plus unchanged full styled grid/cursor for five polls, 200 ms apart',elapsed_ms:performance.now()-settleStart,cursor:settled.cursor};save();
    await shot('repeat-chip-normal');
    if(suffixSpace){send(' ','paste_nav_actual_suffix_space');await shot('suffix-space-normal');}
    paste(pasteNavigationText,'paste_nav_identical_repeat');
    if(suffixSpace) {
      const repeated=await shot('suffix-repeat-two-chips');
      result.chips_after_suffix_repeat=visibleMatches(repeated,'[Pasted').length;save();
      for(let i=1;i<=2;i++) {
        const chip=visibleMatches(await frame(),'[Pasted')[0];
        if(!chip)throw Error('Expected two separate chips after a real suffix space; missing chip '+i);
        send(`\x1b[<0;${chip.x+2};${chip.y+1}M\x1b[<0;${chip.x+2};${chip.y+1}m`,'paste_nav_suffix_click_'+i);
        await sleep(250);await shot('suffix-chip-expanded-'+i);
      }
    }
    const expanded=await shot('repeat-expanded');
    result.chip_fulltext_preserved=[0,1,2].every(i=>expanded.text.includes('VIS11-PASTE-'+i));save();
    for(const [direction,key] of [['up','\x1b[A'],['down','\x1b[B']])for(let i=1;i<=3;i++) {
      send(key,'paste_nav_'+direction+'_'+i);await shot('visual-'+direction+'-'+i);
    }
    const leader=['default','nested'].includes(config)?'\x18':'\x07';
    send(leader,'paste_nav_enter_leader');await shot('repeat-enter-pending');
    send('\r','paste_nav_enter');await sleep(700);await shot('repeat-enter-next');
    await waitFor(f=>f.text.includes('GEOMETRY-SHORT: one short answer.')&&logs.some(e=>e.kind==='provider_completed'&&e.operation==='transcript'),'repeat paste actual completion',15000);
    await sleep(2200);await shot('repeat-enter-restored');
    result.provider_requests=logs.filter(e=>e.kind==='provider').length;
    result.status='OBSERVED';save();return result;
  } catch(error) {
    result.status='FAIL';result.error=String(error);save();await shot('failure-diagnostic').catch(()=>{});return result;
  }
}
