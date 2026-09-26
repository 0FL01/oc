#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
const root=path.resolve(process.argv[2]);
const output=path.join(root,process.argv[3] || 'wheel-summary.json');
if(fs.existsSync(output))throw Error('Refusing to overwrite summary');
const result={root,origins:{},comparisons:[]};
const markers=text=>[...text.matchAll(/(?:SHELL-LINE-\d\d|LIVE-\d-\d\d\d)/g)].map(m=>m[0]);
for(const origin of ['upstream','oc']){
  const dir=path.join(root,origin),checks=JSON.parse(fs.readFileSync(path.join(dir,'wheel-checks.json')));
  result.origins[origin]={status:checks.status,reason:checks.reason,stages:checks.stages.map(s=>{
    const cellsPath=path.join(dir,s.name+'.cells.json');
    const text=fs.existsSync(cellsPath)?JSON.parse(fs.readFileSync(cellsPath)).cells.map(r=>r.map(c=>c.symbol).join('')).join('\n'):'';
    const {observations,endpoint_view,...rest}=s;
    return {...rest,visible_markers:markers(text),visible_list_entries:[...text.matchAll(/ZZ Wheel \d\d/g)].map(m=>m[0]),observation_count:observations?.length};
  })};
  const protocol=JSON.parse(fs.readFileSync(path.join(dir,'protocol.json')));
  const samples=protocol.filter(e=>e.kind==='process_sample');
  if(samples.length){const a=samples[0],b=samples.at(-1);result.origins[origin].process_window={
    duration_ms:(b.at_ns-a.at_ns)/1e6,cpu_ms:1000*(b.cpu_ticks-a.cpu_ticks)/b.clock_ticks_per_second,
    main_thread_voluntary_context_switches:b.voluntary_context_switches-a.voluntary_context_switches,
    main_thread_involuntary_context_switches:b.involuntary_context_switches-a.involuntary_context_switches};}
  const metricsPath=path.join(dir,'scheduler.json');
  if(fs.existsSync(metricsPath)){
    const {frame_samples_ns,...metrics}=JSON.parse(fs.readFileSync(metricsPath));
    const draws=frame_samples_ns.map(s=>s[1]/1e6).sort((a,b)=>a-b);
    result.origins[origin].scheduler={...metrics,sample_count:frame_samples_ns.length,
      draw_mean_ms:metrics.frame_sum_ns/metrics.frame_count/1e6,draw_p50_ms:draws[Math.floor(draws.length*.5)],
      draw_p95_ms:draws[Math.floor(draws.length*.95)],terminal_write_sum_ms:metrics.terminal_write_sum_ns/1e6,
      frame_origin_alignment:'relative to first instrumented draw; no external monotonic epoch in scheduler JSON'};
  }
}
for(const s of result.origins.upstream.stages){
  const native=result.origins.oc.stages.find(n=>n.name===s.name);if(!native)continue;
  if(s.name.startsWith('wheel-'))result.comparisons.push({name:s.name,
    original_markers:s.visible_markers,native_markers:native.visible_markers,
    same_markers:JSON.stringify(s.visible_markers)===JSON.stringify(native.visible_markers),
    original_first_write_ms:s.raw_pty_first_write_ms,native_first_write_ms:native.raw_pty_first_write_ms,
    original_settle_ms:s.raw_pty_last_write_after_last_input_ms,native_settle_ms:native.raw_pty_last_write_after_last_input_ms});
}
fs.writeFileSync(output,JSON.stringify(result,null,2)+'\n');
console.log(JSON.stringify({origins:Object.fromEntries(Object.entries(result.origins).map(([k,v])=>[k,{status:v.status,
  idle:v.stages.filter(s=>s.name.includes('idle')),latency:v.stages.filter(s=>s.name.startsWith('provider-burst')),
  streaming:v.stages.filter(s=>s.name.startsWith('stream-')),scheduler:v.scheduler,process_window:v.process_window}])),
  wheel:result.comparisons.map(({original_markers,native_markers,...s})=>({...s,original_first:original_markers[0],native_first:native_markers[0]}))},null,2));
