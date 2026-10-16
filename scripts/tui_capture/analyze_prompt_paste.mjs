#!/usr/bin/env node
// Current VIS07 behavior/provenance checks; historical VIS11 analyzer is unchanged.
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {expectedPasteDraft} from './prompt_paste.mjs';
const root=path.resolve(process.argv[2]);
const read=p=>JSON.parse(fs.readFileSync(p));
const sha=p=>createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const digest=x=>createHash('sha256').update(JSON.stringify(x)).digest('hex');
const equal=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
const symbols=g=>g.cells.map(r=>r.map(c=>c.symbol).join('')).join('\n');
const userTexts=e=>(e.actual_input||[]).filter(m=>m.role==='user').flatMap(m=>(m.content||[]).filter(c=>c.type==='input_text').map(c=>c.text));
const expectedLong='VIS11 full draft αβ caret-middle preserving every wVIS11 Enter bounded actual requestord';
const result={status:'VIS07_NOT_PASS',scope:'Bounded current-source actual-binary behavior; full unmasked frames; mandatory VIS07 matrix remains open',runs:{},comparisons:{grid:{},png:{}},behavior_checks:[],provenance_checks:[],cursor_differences:0,wire_differences:[]};
const check=(collection,name,ok,observed)=>result[collection].push({name,ok,...(observed===undefined?{}:{observed})});
const runs=fs.readdirSync(root,{withFileTypes:true}).filter(e=>e.isDirectory()&&fs.existsSync(path.join(root,e.name,'capture.lock.json'))).map(e=>e.name).sort();
for(const name of runs) {
  const dir=path.join(root,name),lock=read(path.join(dir,'capture.lock.json'));
  const manifest=read(path.join(dir,'source-manifest.json'));
  const rust=Object.fromEntries(Object.entries(manifest).filter(([p])=>p.startsWith('crates/')||p.startsWith('Cargo')||p==='rust-toolchain.toml'));
  const builds=read(path.join(dir,'commands.json')).filter(c=>equal(c.argv,['cargo','build','--locked']));
  const run=result.runs[name]={exit_code:lock.exit_code,commit:lock.oc.commit,binary_sha256:lock.oc.executable_sha256,rust_inputs_sha256:digest(rust),source_manifest_sha256:lock.oc.source_manifest_sha256,dirty_diff_sha256:lock.oc.dirty_diff_sha256,dimensions:[lock.profile.columns,lock.profile.rows],frames:[],sides:{}};
  check('provenance_checks',name+': fresh build',builds.length===1&&builds[0].exit_code===0,builds.map(b=>b.exit_code));
  check('provenance_checks',name+': unchanged sources during build/capture',lock.oc.source_inputs_unchanged_after_build===true&&lock.oc.source_inputs_unchanged_after_capture===true);
  check('provenance_checks',name+': pinned original',lock.upstream.executable_sha256==='2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a');
  for(const c of lock.captures) {
    const file=path.join(dir,c.path),g=read(file+'.cells.json');
    check('provenance_checks',name+': '+c.path+' sealed cells/PNG/render',sha(file+'.cells.json')===c.cells_sha256&&sha(file+'.png')===c.png_sha256&&sha(file+'.render.json')===c.render_sha256);
    check('provenance_checks',name+': '+c.path+' stable geometry',g.columns===lock.profile.columns&&g.rows===40&&c.status==='CAPTURED');
  }
  for(const a of lock.attempts.filter(a=>a.mode==='grid'||a.mode==='png')) {
    result.comparisons[a.mode][a.status]=(result.comparisons[a.mode][a.status]||0)+1;
    const reportPath=path.join(dir,a.scenario+'.'+a.mode+'-diff.json');
    const d=fs.existsSync(reportPath)?read(reportPath):{};
    const item={scenario:a.scenario,mode:a.mode,status:a.status,different_cells:d.different_cells,different_pixels:d.different_pixels,cursor_differs:d.cursor_differs,bbox:d.difference_bbox_inclusive};
    if(a.mode==='grid'&&d.status!=='INVALID'&&d.status) {
      const up=read(path.join(dir,'upstream',a.scenario+'.cells.json')),oc=read(path.join(dir,'oc',a.scenario+'.cells.json'));
      item.field_counts={};item.rows=[];item.cursors={upstream:up.cursor,oc:oc.cursor};
      for(let y=0;y<up.rows;y++) {
        let count=0;
        for(let x=0;x<up.columns;x++) {
          const u=up.cells[y][x],n=oc.cells[y][x];
          if(!equal(u,n))count++;
          for(const k of ['symbol','fg','bg','modifiers','width'])if(!equal(u[k],n[k]))item.field_counts[k]=(item.field_counts[k]||0)+1;
        }
        if(count)item.rows.push({y,different_cells:count,upstream:up.cells[y].map(c=>c.symbol).join(''),oc:oc.cells[y].map(c=>c.symbol).join('')});
      }
      if(d.cursor_differs)result.cursor_differences++;
    }
    run.frames.push(item);
  }
  for(const side of ['upstream','oc']) {
    const checks=read(path.join(dir,side,'leader-checks.json')),events=read(path.join(dir,side,'protocol.json'));
    const providers=events.filter(e=>e.kind==='provider'),launch=events.find(e=>e.kind==='launch');
    const grid=stage=>read(path.join(dir,side,'leader-'+stage+'.cells.json'));
    const entry=run.sides[side]={status:checks.status,version:launch?.version,provider_requests:providers.map(e=>({operation:e.operation,valid:e.valid,actual_input:e.actual_input,user_texts:userTexts(e)})),provider_completions:events.filter(e=>e.kind==='provider_completed').map(e=>({operation:e.operation})),observations:checks.observations,chip_fulltext_preserved:checks.chip_fulltext_preserved,chip_settlement:checks.chip_settlement,cli_config:events.find(e=>e.kind==='leader_fixture_config')?.cli_config,pending_rgb:{}};
    for(const observation of checks.observations.filter(o=>o.name.includes('pending')||o.name==='normal'||['chip-normal','chip-restored','repeat-expanded'].includes(o.name))) {
      const g=grid(observation.name),draft=[];
      for(let y=0;y<g.rows;y++) {
        const row=g.cells[y].map(c=>c.symbol).join('');
        if(row.includes('VIS11')||row.includes('VIS07')||row.includes('[Pasted')) {
          const colors={};
          for(const c of g.cells[y].filter(c=>c.symbol.trim())){const key=JSON.stringify({fg:c.fg,bg:c.bg,modifiers:c.modifiers});colors[key]=(colors[key]||0)+1;}
          draft.push({y,text:row,styled_nonblank_cells:colors});
        }
      }
      entry.pending_rgb[observation.name]={cursor:g.cursor,rows:draft};
    }
    check('behavior_checks',name+': '+side+' observed scenario',checks.status==='OBSERVED',checks.error);
    check('behavior_checks',name+': '+side+' matched TPS disabled',entry.cli_config?.session?.tps===false);
    check('behavior_checks',name+': '+side+' valid bounded zero-or-two requests',[0,2].includes(providers.length)&&providers.every(e=>e.valid),providers.map(e=>({operation:e.operation,valid:e.valid})));
    if(name.startsWith('chip-')) {
      check('behavior_checks',name+': '+side+' zero requests',providers.length===0);
      check('behavior_checks',name+': '+side+' actual click expands original 3 lines',checks.chip_fulltext_preserved===true);
      const normal=grid('chip-normal'),pending=grid('chip-pending'),restored=grid('chip-restored');
      check('behavior_checks',name+': '+side+' chip normal/pending/restored preserve symbols/cursor',symbols(normal)===symbols(pending)&&symbols(normal)===symbols(restored)&&equal(normal.cursor,pending.cursor)&&equal(normal.cursor,restored.cursor));
      entry.chip_cells=Object.fromEntries(['chip-normal','chip-pending','chip-restored'].map(stage=>{
        const g=grid(stage),cells=[];
        for(let y=0;y<g.rows;y++)for(let x=0;x<g.columns;x++)if(g.cells[y][x].symbol==='['&&g.cells[y].slice(x,x+7).map(c=>c.symbol).join('')==='[Pasted')cells.push({x,y,cell:g.cells[y][x]});
        return [stage,cells];
      }));
      check('behavior_checks',name+': '+side+' chip label observed bold in all three states',Object.values(entry.chip_cells).every(c=>c.length===1&&c[0].cell.modifiers.includes('bold')),entry.chip_cells);
    } else {
      const main=providers.filter(e=>e.operation==='transcript'),title=providers.filter(e=>e.operation==='title');
      const navigation=name.startsWith('repeat-nav-')||name.startsWith('suffix-space-');
      const expected=navigation?expectedPasteDraft(side,name.startsWith('suffix-space-')):expectedLong;
      check('behavior_checks',name+': '+side+' one main plus one title',main.length===1&&title.length===1,providers.length);
      check('behavior_checks',name+': '+side+' exact original raw draft on wire',main.length===1&&equal(userTexts(main[0]),[expected]),main.map(userTexts));
      if(navigation) {
        check('behavior_checks',name+': '+side+' completed main plus title and stable restored frame',entry.provider_completions.filter(e=>e.operation==='transcript').length===1&&entry.provider_completions.filter(e=>e.operation==='title').length===1&&checks.observations.some(o=>o.name==='repeat-enter-restored'&&o.status==='CAPTURED'),entry.provider_completions);
        check('behavior_checks',name+': '+side+' raw VT contains no Rust panic',!fs.readFileSync(path.join(dir,side,'raw.vt'),'utf8').includes('panicked at'));
        check('behavior_checks',name+': '+side+' repeated paste exact raw bytes',checks.chip_fulltext_preserved===true&&main.length===1&&equal(userTexts(main[0]),[expected]));
        if(name.startsWith('suffix-space-'))check('behavior_checks',name+': '+side+' real suffix space prevents repeat expansion and creates two actual chips',checks.suffix_space===true&&checks.chips_after_suffix_repeat===2,checks.chips_after_suffix_repeat);
        else check('behavior_checks',name+': '+side+' raw-end repeat expands without duplicate insert',main.length===1&&equal(userTexts(main[0]),[expected]));
        entry.visual_navigation=checks.observations.filter(o=>o.name.startsWith('visual-')).map(o=>({name:o.name,cursor:o.cursor}));
        check('behavior_checks',name+': '+side+' six actual visual navigation samples',entry.visual_navigation.length===6);
        check('behavior_checks',name+': '+side+' Up moves then Down restores exact cursor',entry.visual_navigation.length===6&&!equal(entry.visual_navigation[0].cursor,checks.observations.find(o=>o.name==='repeat-expanded')?.cursor)&&equal(entry.visual_navigation.at(-1).cursor,checks.observations.find(o=>o.name==='repeat-expanded')?.cursor),entry.visual_navigation);
      }
    }
  }
  const up=run.sides.upstream.provider_requests.find(e=>e.operation==='transcript')?.user_texts;
  const oc=run.sides.oc.provider_requests.find(e=>e.operation==='transcript')?.user_texts;
  if(up&&oc&&!equal(up,oc))result.wire_differences.push({run:name,upstream:up,oc,exact_equal:false,source:'opencode/packages/tui/src/component/prompt/index.tsx:1393,1422–1424'});
}
check('provenance_checks','all runs use one native executable',new Set(Object.values(result.runs).map(r=>r.binary_sha256)).size===1);
check('provenance_checks','all runs use one Rust input set',new Set(Object.values(result.runs).map(r=>r.rust_inputs_sha256)).size===1);
check('behavior_checks','default-chip and extra-longdraft at all four boundaries',[79,80,120,121].every(w=>result.runs['chip-'+w]&&result.runs['extra-'+w]));
check('behavior_checks','repeated paste/navigation main profile captured',!!result.runs['repeat-nav-120']);
if(fs.existsSync(path.join(root,'campaign.json'))&&read(path.join(root,'campaign.json')).runs.some(r=>r.suffixSpace))check('behavior_checks','suffix-space false repeat case captured',!!result.runs['suffix-space-120']);
result.behavior_failures=result.behavior_checks.filter(c=>!c.ok);
result.provenance_failures=result.provenance_checks.filter(c=>!c.ok);
// Only this explicitly diagnosed fixture-only correction may select a fresh
// attempt. The rejected wire, timeout and all differences remain in the report.
const currentNavigation=result.runs['repeat-nav-120-03']?'repeat-nav-120-03':result.runs['repeat-nav-120-02']?'repeat-nav-120-02':'repeat-nav-120';
result.current_runs=runs.filter(name=>!name.startsWith('repeat-nav-')||name===currentNavigation);
const retained=c=>runs.some(name=>name.startsWith('repeat-nav-')&&name!==currentNavigation&&c.name.startsWith(name+':'));
result.retained_attempt_failures=result.behavior_failures.filter(retained);
result.current_behavior_failures=result.behavior_failures.filter(c=>!retained(c));
result.behavior_status=result.current_behavior_failures.length?'BOUNDED_BEHAVIOR_FAILED':result.wire_differences.length?'BOUNDED_BEHAVIOR_VERIFIED_WITH_WIRE_DIFFERENCE':'BOUNDED_BEHAVIOR_VERIFIED';
fs.writeFileSync(path.join(root,process.argv[3]||'prompt-paste-analysis.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({status:result.status,behavior_status:result.behavior_status,runs:runs.length,comparisons:result.comparisons,cursor_differences:result.cursor_differences,current_behavior_failures:result.current_behavior_failures,retained_attempt_failures:result.retained_attempt_failures,provenance_failures:result.provenance_failures,wire_differences:result.wire_differences},null,2));
process.exitCode=result.current_behavior_failures.length||result.provenance_failures.length?1:0;
