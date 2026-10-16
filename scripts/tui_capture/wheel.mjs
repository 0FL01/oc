import fs from 'node:fs';
import path from 'node:path';

// Endpoint parity is evaluated separately from motion: the pinned original
// ScrollBox applies the default three-row displacement immediately; native
// intentionally approaches that endpoint on active 16.667ms scroll deadlines.
export async function probeWheel({origin,dir,send,waitFor,frame,capture,visibleMatches,logs,outputTimeline,control,prompt,resize}) {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  const checks={origin,status:'IN_PROGRESS',stages:[],motion_contract:'compare settled endpoints; temporal frames may differ; rates are requested input cadence, not physical FPS'};
  const save=()=>fs.writeFileSync(path.join(dir,'wheel-checks.json'),JSON.stringify(checks,null,2)+'\n');
  const symbols=f=>f.cells.map(row=>row.map(c=>c.symbol).join(''));
  const view=f=>symbols(f).slice(2,-8).join('\n');
  const sample=async label=>{
    control('process_sample',{request_id:label});
    for(let i=0;i<100;i++){const s=logs.find(e=>e.kind==='process_sample'&&e.request_id===label);if(s)return s;await sleep(10);}
    throw Error('process sample timeout');
  };
  const shot=async(name,f,extra={})=>{await capture(name,f,'CAPTURED_BOUNDED');checks.stages.push({name,cursor:f.cursor,...extra});save();};
  const idle=async label=>{
    await sleep(250);
    const a=await sample(label+'-start'), n=outputTimeline.length;
    await sleep(1000);
    const b=await sample(label+'-end');
    const bytes=outputTimeline.slice(n).reduce((s,e)=>s+e.bytes,0);
    checks.stages.push({name:label,start_ns:a.at_ns,end_ns:b.at_ns,window_ms:(b.at_ns-a.at_ns)/1e6,cpu_ticks:b.cpu_ticks-a.cpu_ticks,
      cpu_ms:1000*(b.cpu_ticks-a.cpu_ticks)/b.clock_ticks_per_second,terminal_bytes:bytes,
      main_thread_voluntary_context_switches:b.voluntary_context_switches-a.voluntary_context_switches,
      main_thread_involuntary_context_switches:b.involuntary_context_switches-a.involuntary_context_switches});save();
  };
  const wheel=async(name,events,hz,x=20,y=12)=>{
    const before=await frame(), start=performance.now(), base=outputTimeline.length;
    const observations=[], sent=[];
    let finished=false;
    const reader=(async()=>{while(!finished){const f=await frame();observations.push({at_ms:performance.now()-start,view:view(f),cursor:f.cursor});await sleep(2);}})();
    control('wheel_sequence',{request_id:name,events,hz,x,y});
    for(let i=0;i<1000&&!logs.some(e=>e.kind==='paced_input_done'&&e.request_id===name);i++)await sleep(2);
    const injection=logs.filter(e=>e.kind==='paced_input'&&e.request_id===name);
    if(injection.length!==events.length)throw Error('wheel injection incomplete');
    for(const e of injection)sent.push({at_ms:(e.at_ns-injection[0].at_ns)/1e6,direction:events[e.index]});
    let previous='',lastChange=performance.now(),endpoint;
    const deadline=performance.now()+3000;
    while(performance.now()<deadline){endpoint=await frame();const k=view(endpoint);if(k!==previous){previous=k;lastChange=performance.now();}if(performance.now()-lastChange>=150)break;await sleep(4);}
    finished=true;await reader;
    const changed=observations.filter((o,i)=>i===0?o.view!==view(before):o.view!==observations[i-1].view);
    const writes=outputTimeline.slice(base).filter(e=>e.at_ns>=injection[0].at_ns);
    await shot(name,endpoint,{requested_hz:hz,sent,raw_pty_first_write_ms:writes.length?(writes[0].at_ns-injection[0].at_ns)/1e6:null,
      raw_pty_last_write_after_last_input_ms:writes.length?(writes.at(-1).at_ns-injection.at(-1).at_ns)/1e6:null,
      frontend_observation_only:true,first_changed_cell_ms:changed[0]?.at_ms??null,
      last_changed_cell_ms:changed.at(-1)?.at_ms??null,settle_after_last_input_ms:changed.length?changed.at(-1).at_ms-sent.at(-1).at_ms:null,
      terminal_chunks:outputTimeline.length-base,endpoint_view:view(endpoint),cursor_before:before.cursor,
      editor_cursor_preserved:JSON.stringify(before.cursor)===JSON.stringify(endpoint.cursor),observations});
    return endpoint;
  };
  try {
    send('/agents');await waitFor(f=>f.text.includes('/agents'),'agent draft');send('\r');await waitFor(f=>f.text.includes('Select agent'),'agents');send('fixture-shell');await waitFor(f=>f.text.includes('fixture-shell'),'profile');send('\r');await waitFor(f=>f.text.includes('Fixture-Shell ·')&&!f.text.includes('Select agent'),'profile selected');
    send('\x1b[200~'+prompt+'\x1b[201~');send('\r');
    const done=await waitFor(f=>f.text.includes('BOUNDED-SHELL:')&&logs.filter(e=>e.kind==='provider_completed').length===4,'real Shell completion');
    const requests=logs.filter(e=>e.kind==='provider'&&e.operation==='transcript');
    if(requests.length!==3||requests.some(e=>!e.valid)||!requests.at(-1).fixture_outputs_verified)throw Error('real Shell fixture rejected');
    await shot('wheel-shell-completed',done,{verified_real_outputs:true});
    const hits=visibleMatches(done,origin==='upstream'?"$ printf 'SHELL-LINE-%02d":'$ printf SHELL-LINE-%02d');
    if(hits.length!==1)throw Error('long Shell header not unique');
    const {x,y}=hits[0];send(`\x1b[<0;${x+3};${y+1}M`);send(`\x1b[<0;${x+3};${y+1}m`);
    await resize(80);await waitFor(f=>f.rows===80&&f.text.includes('SHELL-LINE-01')&&f.text.includes('SHELL-LINE-40'),'expanded actual Shell');
    await resize(40);await waitFor(f=>f.rows===40&&f.text.includes('SHELL-LINE-40'),'expanded Shell 40 rows');
    send('\x1b[200~FOCUS-A\nFOCUS-B\x1b[201~');await waitFor(f=>f.text.includes('FOCUS-A')&&f.text.includes('FOCUS-B'),'multiline draft');
    await idle('expanded-idle');
    for(const hz of [165,250]){
      await wheel(`wheel-${hz}-single-up`,[-1],hz);
      await wheel(`wheel-${hz}-burst-up`,Array(8).fill(-1),hz);
      await wheel(`wheel-${hz}-reverse`,[-1,-1,-1,1,1,1],hz);
      await wheel(`wheel-${hz}-top-edge`,Array(32).fill(-1),hz);
      await wheel(`wheel-${hz}-bottom-edge`,Array(32).fill(1),hz);
      await idle(`settled-${hz}-idle`);
    }
    send('\x03');await sleep(100);
    send('/models');await waitFor(f=>f.text.includes('/models'),'models draft');send('\r');const modal=await waitFor(f=>f.text.includes('Select model'),'models');
    await shot('wheel-models-before',modal);await wheel('wheel-models-up',[-1,-1,-1,-1],250,60,20);await wheel('wheel-models-down',[1,1,1,1],250,60,20);send('\x1b');await waitFor(f=>!f.text.includes('Select model'),'models dismissed');
    for(const index of [3,4]){
      send('\x1b[200~'+prompt+'\x1b[201~');send('\r');
      for(let i=0;i<1000&&!logs.some(e=>e.kind==='wheel_stream_held'&&e.index===index);i++)await sleep(10);
      if(!logs.some(e=>e.kind==='wheel_stream_held'&&e.index===index))throw Error('stream not held');
      await sleep(250);
      if(index===3)await wheel('wheel-stream-detach',Array(12).fill(-1),165);
      const before=await frame();await shot(index===3?'stream-detached-before':'stream-sticky-before',before);
      control('release_wheel');
      // Keep detached-anchor qualification independent of keyboard edits:
      // typing itself can deliberately reveal the prompt/bottom on native.
      if(index===4)for(const hz of [165,250]){
      const request_id='provider-burst-'+hz;
      const base=outputTimeline.length;
      control('glyph_sequence',{request_id,hz});
      for(let i=0;i<1000&&!logs.some(e=>e.kind==='paced_input_done'&&e.request_id===request_id);i++)await sleep(2);
      await sleep(25);
      checks.pending_latency ??= [];
      checks.pending_latency.push({request_id,hz,base});
      }
      await waitFor(f=>logs.filter(e=>e.kind==='provider_completed').length===index+2,'stream complete');
      await sleep(500);
      for(const {request_id,hz,base} of checks.pending_latency??[]){
      const injected=logs.filter(e=>e.kind==='paced_input'&&e.request_id===request_id);
      const latencies=injected.map(e=>{
        const glyph=Buffer.from(e.base64,'base64');
        const output=outputTimeline.slice(base).find(o=>o.at_ns>=e.at_ns&&Buffer.from(o.base64,'base64').includes(glyph));
        return output?(output.at_ns-e.at_ns)/1e6:null;
      });
      const sorted=latencies.filter(v=>v!==null).sort((a,b)=>a-b);
      checks.stages.push({name:request_id,requested_hz:hz,injected_count:injected.length,painted_count:sorted.length,
        utf8_event_to_pty_paint_ms:latencies,p50_ms:sorted.length===32?sorted[16]:null,p95_ms:sorted.length===32?sorted[30]:null,max_ms:sorted.at(-1)??null});save();
      }
      delete checks.pending_latency;
      const after=await frame();await shot(index===3?'stream-detached-after':'stream-sticky-after',after,{anchor_unchanged:view(before)===view(after),last_live_row_visible:after.text.includes(`LIVE-${index}-119`)});
      await wheel('wheel-stream-'+index+'-repin',Array(64).fill(1),250);
      if(index===4){send('\x03');await sleep(100);}
    }
    await idle('final-settled-idle');
    send('\x03','wheel-clean-quit');
    for(let i=0;i<500&&!logs.some(e=>e.kind==='exit');i++)await sleep(10);
    checks.exit=logs.find(e=>e.kind==='exit');
    if(!checks.exit||checks.exit.code!==0||checks.exit.termination!=='natural')throw Error('clean exit failed');
    checks.status='MEASURED';save();return checks;
  }catch(e){checks.status='FAILED';checks.reason=e.message;save();throw e;}
}
