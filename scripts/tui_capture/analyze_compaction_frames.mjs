#!/usr/bin/env node
// Derivative measurements only: full grids and PNGs remain unmodified/unmasked.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const base=path.resolve(process.argv[2]||'evidence/tui/recovery-v00');
const read=p=>JSON.parse(fs.readFileSync(p,'utf8'));
const report={scope:'Full 120x40 styled cells; full decoded PNG comparator counts. Field counts overlap; no causal allocation or masking of dynamic cells.',campaigns:{}};
const latest=process.argv.includes('--latest');
const optional=process.argv.includes('--optional');
if(optional&&!latest)throw Error('--optional requires --latest');
const final=latest||process.argv.includes('--final');
for(const suffix of latest?(optional?['18','19','20','21','22']:['18','19','20']):final?['15','16','17']:['12','13','14']) {
  const dir=path.join(base,'compaction20260927-'+suffix),lock=read(path.join(dir,'capture.lock.json'));
  const frames=[];
  for(const a of lock.attempts.filter(a=>a.mode==='grid')) {
    const name=a.scenario,up=read(path.join(dir,'upstream',name+'.cells.json')),oc=read(path.join(dir,'oc',name+'.cells.json'));
    const grid=read(path.join(dir,name+'.grid-diff.json')),png=read(path.join(dir,name+'.png-diff.json'));
    let changed=0,symbol=0,styleOnly=0;const fields={},rowCounts=Array(40).fill(0);
    for(let y=0;y<40;y++)for(let x=0;x<120;x++) {
      const u=up.cells[y][x],n=oc.cells[y][x];const diff=[...new Set([...Object.keys(u),...Object.keys(n)])].filter(k=>JSON.stringify(u[k])!==JSON.stringify(n[k]));
      if(diff.length){changed++;rowCounts[y]++;if(diff.includes('symbol'))symbol++;else styleOnly++;for(const k of diff)fields[k]=(fields[k]||0)+1;}
    }
    assert.equal(changed,grid.different_cells);
    const rows=g=>g.cells.map(r=>r.map(c=>c.symbol).join(''));
    const positions=g=>Object.fromEntries(['Compaction · 1.2K','Objective','VIS34-CHECKPOINT','Work State','Three seeded','Next Move','Continue the user request','VIS34 held tool:','VIS34 next:','VIS34-HELD-DONE','VIS34-NEXT-DONE'].map(s=>[s,rows(g).flatMap((r,i)=>r.includes(s)?[i+1]:[])]));
    const r={scenario:name,different_cells:changed,symbol_different:symbol,style_only_different:styleOnly,field_counts:fields,different_cells_by_row_1based:rowCounts,different_pixels:png.different_pixels,pixels_checked:png.pixels_checked,positions_1based:{upstream:positions(up),oc:positions(oc)}};
    if(final&&name.endsWith('completed'))r.summary_divider_and_body_rows_16_through_29_different_cells=rowCounts.slice(15,29).reduce((a,n)=>a+n,0);
    frames.push(r);
  }
  report.campaigns[suffix]={native_sha256:lock.oc.executable_sha256,frames};
}
fs.writeFileSync(path.join(base,optional?'compaction-visual-measurements-latest-options.json':latest?'compaction-visual-measurements-latest.json':final?'compaction-visual-measurements-final.json':'compaction-visual-measurements.json'),JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify(Object.fromEntries(Object.entries(report.campaigns).map(([run,c])=>[run,c.frames.filter(f=>/completed|running-1|threshold-running|overflow-running/.test(f.scenario)).map(f=>process.argv.includes('--quiet')?{scenario:f.scenario,different_cells:f.different_cells,different_pixels:f.different_pixels,summary_cells:f.summary_divider_and_body_rows_16_through_29_different_cells}:f)])),null,2));
