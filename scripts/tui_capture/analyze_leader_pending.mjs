#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
const root=path.resolve(process.argv[2]);
const read=p=>JSON.parse(fs.readFileSync(p));
const sha=p=>createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const result={status:'ORIGINAL_OBSERVED_NATIVE_PAIR_PENDING',runs:{}};
for(const mode of ['default','configured','legacy-v1','precedence']) {
  const dir=path.join(root,mode+'-final'),up=path.join(dir,'upstream');
  const lock=read(path.join(dir,'capture.lock.json')),checks=read(path.join(up,'leader-checks.json'));
  assert.equal(checks.provider_requests,0);assert.equal(checks.chip_fulltext_preserved,true);
  const grids=Object.fromEntries(lock.captures.map(c=>{
    const file=path.join(root,mode+'-final',c.path);
    assert.equal(sha(file+'.cells.json'),c.cells_sha256);assert.equal(sha(file+'.png'),c.png_sha256);
    return [c.scenario.replace('leader-',''),read(file+'.cells.json')];
  }));
  const symbols=g=>g.cells.map(row=>row.map(c=>c.symbol).join('')).join('\n');
  const preservation={};
  for(const name of ['escape','backspace','repeated']) {
    assert.equal(symbols(grids[name+'-pending']),symbols(grids[name+'-next']));
    assert.deepEqual(grids[name+'-pending'].cursor,grids[name+'-next'].cursor);
    preservation[name]={full_grid_symbols_equal:true,cursor_equal:true};
  }
  assert.equal(symbols(grids['idle-pending']),symbols(grids['idle-restored']));
  assert.deepEqual(grids['idle-pending'].cursor,grids['idle-restored'].cursor);
  assert.equal(symbols(grids['chip-normal']),symbols(grids['chip-restored']));
  assert.deepEqual(grids['chip-normal'].cursor,grids['chip-restored'].cursor);
  const normal=grids.normal,pending=grids['invalid-pending'];
  assert.equal(symbols(normal),symbols(pending));assert.deepEqual(normal.cursor,pending.cursor);
  const color_changes=[];
  normal.cells.forEach((row,y)=>row.forEach((c,x)=>{const p=pending.cells[y][x];if(c.fg!==p.fg||c.bg!==p.bg)color_changes.push({x,y,symbol:c.symbol,normal_fg:c.fg,pending_fg:p.fg,normal_bg:c.bg,pending_bg:p.bg});}));
  result.runs[mode]={binary_sha256:lock.upstream.executable_sha256,provider_requests:0,
    idle_observed_interval_ms:checks.idle.samples.slice(-2).map(x=>x.elapsed_ms),
    preservation,chip_fulltext_preserved:true,color_changes,
    captures:lock.captures.length,unstable:lock.captures.filter(c=>c.status==='UNSTABLE_CAPTURE').map(c=>c.scenario),
    observations:checks.observations};
}
fs.writeFileSync(path.join(root,'original-analysis.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify(Object.fromEntries(Object.entries(result.runs).map(([k,v])=>[k,{captures:v.captures,requests:v.provider_requests,idle_interval:v.idle_observed_interval_ms,unstable:v.unstable}]))));
