// Observational oracle: real keys into the original, full unmasked VT/PNG output.
import fs from 'node:fs';
import path from 'node:path';
export async function probeLeaderPending({origin,dir,send,frame,capture,visibleMatches,logs,config,sampleColor,extra=false,enterOnly=false,outputTimeline}) {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  const result={origin,config,status:'IN_PROGRESS',observations:[]};
  const save=()=>fs.writeFileSync(path.join(dir,'leader-checks.json'),JSON.stringify(result,null,2)+'\n');
  const leader=['default','nested'].includes(config)?'\x18':'\x07';
  const defaultChord=['default','nested'].includes(config);
  const draft='VIS11 full draft αβ caret-middle preserving every word';
  const shot=async name=>{await sleep(90);const f=await frame();const status=await capture('leader-'+name,f,'CAPTURED');const p=visibleMatches(f,'VIS11')[0];result.observations.push({name,status,at_ms:performance.now(),cursor:f.cursor,draft_row:p?f.cells[p.y].map(c=>c.symbol).join('').trim():null,draft_cell:p?f.cells[p.y][p.x]:null,commands:f.text.includes('Commands'),provider_requests:logs.filter(e=>e.kind==='provider').length});save();return f;};
  try {
    send('\x14','leader_variant_cycle');await sleep(200);
    send('\x1b[200~'+draft+'\x1b[201~','leader_draft');await sleep(200);
    send('\x1b[D\x1b[D\x1b[D','leader_caret_middle');
    const normal=await shot('normal');const p=visibleMatches(normal,'VIS11')[0];if(!p)throw Error('Actual composer draft missing');
    if(enterOnly) {
      send(leader,'leader_enter_only_start');await shot('enter-only-pending');send('\r','leader_enter_only_next');
      await sleep(700);await shot('enter-only-next');await sleep(2200);await shot('enter-only-restored');
      result.provider_requests=logs.filter(e=>e.kind==='provider').length;result.status='OBSERVED';save();return result;
    }
    if(extra) {
      for(const [name,key] of [['left','\x1b[D'],['right','\x1b[C']]) {
        send(leader,'leader_extra_'+name+'_start');await shot('extra-'+name+'-pending');
        send(key,'leader_extra_'+name+'_next');await sleep(450);await shot('extra-'+name+'-next');
        await sleep(2200);await shot('extra-'+name+'-restored');
      }
      send('\x10','leader_extra_modal_open');await sleep(450);await shot('extra-modal-normal');
      send('zzzz-no-such-command','leader_extra_modal_filter');await sleep(400);await shot('extra-modal-filtered');
      send(leader,'leader_extra_modal_start');const started=performance.now();await shot('extra-modal-pending');
      await sleep(2200);await shot('extra-modal-expired');result.modal_idle={duration_ms:performance.now()-started};
      send('\x1b','leader_extra_modal_close');await sleep(450);await shot('extra-modal-restored');
      send(leader,'leader_extra_ctrlc_start');await shot('extra-ctrlc-pending');
      send('\x03','leader_extra_ctrlc_next');await sleep(450);await shot('extra-ctrlc-next');
      await sleep(2200);await shot('extra-ctrlc-restored');
      send('\x1b[200~VIS11 Enter bounded actual request\x1b[201~','leader_extra_enter_draft');await sleep(400);
      send(leader,'leader_extra_enter_start');await shot('extra-enter-pending');send('\r','leader_extra_enter_next');
      await sleep(700);await shot('extra-enter-next');await sleep(2200);await shot('extra-enter-restored');
      result.provider_requests=logs.filter(e=>e.kind==='provider').length;result.status='OBSERVED';save();return result;
    }
    for(const [name,key] of [['invalid','z'],['escape','\x1b'],['backspace','\x7f'],['repeated',leader]]) {
      send(leader,'leader_'+name+'_start');await shot(name+'-pending');
      send(key,'leader_'+name+'_next');await shot(name+'-next');await sleep(2200);await shot(name+'-restored');
    }
    const idleInputStart=logs.length,idleOutputStart=outputTimeline.length;
    send(leader,'leader_idle_start');const started=performance.now();await shot('idle-pending');
    const samples=[];let expiry=null;
    while(performance.now()-started<3200){const color=await sampleColor(p);const elapsed=performance.now()-started;const pending=color!==normal.cells[p.y][p.x].fg;samples.push({elapsed_ms:elapsed,pending});if(!pending){expiry=elapsed;break;}await sleep(25);}
    result.idle={expiry_ms:expiry,samples};save();await shot('idle-restored');
    result.idle.actual_inputs=logs.slice(idleInputStart).filter(e=>e.kind==='input_written');
    result.idle.actual_output_writes=outputTimeline.slice(idleOutputStart).map(e=>({at_ns:e.at_ns,bytes:e.bytes}));save();
    // Configured chord demonstrates effective remapping, defaults use ctrl+p.
    send(leader,'leader_success_start');await shot('success-pending');
    await sleep(2200);send(leader+(defaultChord?'m':'p'),'leader_success_fresh_chord');await sleep(450);await shot('success-next');
    send('\x1b','leader_success_dismiss');await sleep(250);await shot('success-restored');
    if(!defaultChord) {send(leader,'leader_modal_open_start');await sleep(80);send('p','leader_modal_open_chord');}else send('\x10','leader_modal_open');await sleep(450);await shot('modal-normal');
    send(leader,'leader_modal_start');await shot('modal-pending');
    send('z','leader_modal_invalid');await shot('modal-invalid-next');
    send('\x1b','leader_modal_escape');await sleep(450);await shot('modal-restored');
    // Real summarized paste chip; do not inject a chip into the renderer.
    send('\x1b[F','leader_end');send('\x1b[200~'+Array.from({length:3},(_,i)=>'VIS11-PASTE-'+i).join('\n')+'\x1b[201~','leader_chip_paste');
    await shot('chip-normal');send(leader,'leader_chip_start');await shot('chip-pending');await sleep(2200);await shot('chip-restored');
    const chipFrame=await frame(),chip=visibleMatches(chipFrame,'[Pasted')[0];
    if(chip){send(`\x1b[<0;${chip.x+2};${chip.y+1}M\x1b[<0;${chip.x+2};${chip.y+1}m`,'leader_expand_actual_chip');await sleep(250);const expanded=await shot('chip-expanded-fulltext');result.chip_fulltext_preserved=[0,1,2].every(i=>expanded.text.includes('VIS11-PASTE-'+i));}
    result.provider_requests=logs.filter(e=>e.kind==='provider').length;
    result.status=result.provider_requests===0?'OBSERVED':'FAIL_PROVIDER_REQUEST';save();return result;
  } catch(error) {result.status='FAIL';result.error=String(error);save();await shot('failure-diagnostic').catch(()=>{});return result;}
}
