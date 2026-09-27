#!/usr/bin/env node
// Read-only replay of actual PTY chunks through the same pinned xterm parser.
// No synthesized timestamps, phase alignment, screenshot alteration or DB writes.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const here=path.dirname(fileURLToPath(import.meta.url));
const base=path.resolve(process.argv[2]||'evidence/tui/recovery-v00');
const dir=path.join(base,'compaction20260927-22');
const tools='/home/opencode/.cache/opencode-tmp/opencode/t44-reference';
process.env.PLAYWRIGHT_BROWSERS_PATH=path.join(tools,'browsers');
const require=createRequire(path.join(tools,'package.json'));
const {chromium}=require('playwright');
const browser=await chromium.launch({headless:true,env:{HOME:tools,PATH:'/usr/bin:/bin',LANG:'C.UTF-8'}});
const report={scope:'Actual generation-0 PTY read chunks replayed through xterm; Compaction row glyph changes only after partial summary appears. Chunk timestamps are original bridge monotonic_ns, not application paint timestamps. Pause/resume gaps remain included. No exact 80ms deadline/FPS claim.',sides:{}};
try {
  for(const origin of ['upstream','oc']) {
    const file=path.join(dir,origin,'output-timeline.jsonl'),bytes=fs.readFileSync(file);
    const timeline=bytes.toString().trim().split('\n').map(JSON.parse);
    const page=await browser.newPage();
    await page.setContent('<div id="terminal"></div>');
    for(const script of ['node_modules/@xterm/xterm/lib/xterm.js','node_modules/@xterm/addon-unicode11/lib/addon-unicode11.js'])await page.addScriptTag({path:path.join(tools,script)});
    await page.addScriptTag({path:path.join(here,'frontend.js')});
    await page.exposeFunction('terminalReply',()=>{});
    await page.evaluate(()=>startTerminal({columns:120,rows:40}));
    const changes=await page.evaluate(async events=>{
      const out=[],after=[];let previous='',completedAt=null,lastAt=null;
      for(const [index,e] of events.entries()) {
        if(e.generation!==0)break;
        lastAt=e.at_ns;
        await writeTerminal(e.base64);
        const buffer=term.buffer.active,rows=Array.from({length:40},(_,y)=>buffer.getLine(y)?.translateToString(false)||'');
        const row=rows.findIndex(r=>r.includes('Compaction'));
        if(row<0||!rows.some(r=>r.includes('VIS34-CHECKPOINT')))continue;
        const glyph=rows[row].match(/[\u2800-\u28ff]/u)?.[0]||'';
        const completed=rows[row].includes('1.2K in');
        if(completedAt!==null){if(glyph)after.push({timeline_index:index,at_ns:e.at_ns,glyph});continue;}
        if(glyph&&glyph!==previous){out.push({timeline_index:index,at_ns:e.at_ns,row_1based:row+1,glyph,row_text:rows[row]});previous=glyph;}
        if(completed){out.push({timeline_index:index,at_ns:e.at_ns,row_1based:row+1,glyph:'',row_text:rows[row],completed:true});completedAt=e.at_ns;}
      }
      return {changes:out,post_completion_braille:after,post_completion_observed_ms:(lastAt-completedAt)/1e6};
    },timeline);
    const phases=changes.changes.filter(c=>c.glyph),deltas=phases.slice(1).map((c,i)=>(c.at_ns-phases[i].at_ns)/1e6),sorted=[...deltas].sort((a,b)=>a-b);
    assert(phases.length>2&&new Set(phases.map(c=>c.glyph)).size>1);assert(changes.changes.at(-1).completed);assert.equal(changes.post_completion_braille.length,0);
    const samples=JSON.parse(fs.readFileSync(path.join(dir,origin,'compaction-animation-samples.json'))).samples;
    const protocol=JSON.parse(fs.readFileSync(path.join(dir,origin,'protocol.json')));
    report.sides[origin]={timeline_sha256:createHash('sha256').update(bytes).digest('hex'),chunks:timeline.length,phase_observations:phases.length,transitions:deltas.length,phases:[...new Set(phases.map(c=>c.glyph))],
      delta_ms:{min:sorted[0],median:sorted[Math.floor(sorted.length/2)],max:sorted.at(-1),all:deltas},
      live_browser_samples:samples.length,live_sample_window_ms:samples.at(-1).observed_ms-samples[0].observed_ms,
      paused_full_grid_png:protocol.filter(e=>['scanner_pause_ack','scanner_resume_ack'].includes(e.kind)),...changes};
    await page.close();
  }
} finally {await browser.close();}
fs.writeFileSync(path.join(base,'compaction-animation-observations.json'),JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify(Object.fromEntries(Object.entries(report.sides).map(([side,s])=>[side,{chunks:s.chunks,transitions:s.transitions,phases:s.phases,delta_ms:{min:s.delta_ms.min,median:s.delta_ms.median,max:s.delta_ms.max},live_browser_samples:s.live_browser_samples,post_completion_braille:s.post_completion_braille.length,post_completion_observed_ms:s.post_completion_observed_ms}])),null,2));
