#!/usr/bin/env node
// Integrity, temporal observation and exact unmasked pairing. Never turns source
// fixtures, missing native captures, or unmatched glyph phases into pixel PASS.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createGunzip,constants} from 'node:zlib';
import {spawnSync} from 'node:child_process';
import readline from 'node:readline';
import {fileURLToPath} from 'node:url';

const here=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(here,'../..');
const args={};for(let i=2;i<process.argv.length;i+=2){assert(process.argv[i].startsWith('--')&&process.argv[i+1]);args[process.argv[i].slice(2)]=process.argv[i+1];}
for(const key of Object.keys(args))assert(['reference','native','output','diagnostic','inventory'].includes(key),'Unknown option '+key);
const output=path.resolve(args.output||'');assert.equal(path.dirname(output),path.join(repo,'evidence/tui/recovery-v00'));assert(!fs.existsSync(output),'Immutable report already exists');
const sha=value=>createHash('sha256').update(value).digest('hex');
const report={scope:'T44 VIS41 / VIS39 own-running only',capture_gate:'PENDING_NATIVE_PAIRED_CAPTURE',native_idle_counter_gate:'PENDING_FRESH_NATIVE_ACTUAL_IDLE_COUNTERS',sides:{},comparisons:[],failures:[],failed_attempt_history:[],method:'Real captured styled cells, PNG and cursor; actual observed title/glyph phases, no global time synchronization, normalization, mask/crop, or tolerance. Comma-separated roots are ordered explicitly; the last sealed attempt of each profile is current and all previous results remain reported.'};
if(args.inventory){
  report.inventory=[];
  for(const root of args.inventory.split(',').map(p=>path.resolve(p))){
    const files={};
    const walk=dir=>{for(const entry of fs.readdirSync(dir,{withFileTypes:true})){const name=path.join(dir,entry.name);if(entry.isDirectory())walk(name);else if(entry.isFile())files[path.relative(root,name)]={sha256:sha(fs.readFileSync(name)),bytes:fs.statSync(name).size};else throw Error('Unsupported inventory entry '+name);}};
    if(fs.statSync(root).isFile())files[path.basename(root)]={sha256:sha(fs.readFileSync(root)),bytes:fs.statSync(root).size};else walk(root);
    report.inventory.push({root,files,total_bytes:Object.values(files).reduce((n,f)=>n+f.bytes,0),qualification:'Byte inventory seal includes interrupted and rejected attempts; existence/integrity never implies valid capture or parity'});
  }
}
const seal=dir=>{
  const manifest=JSON.parse(fs.readFileSync(path.join(dir,'seal.json')));
  for(const [file,hash]of Object.entries(manifest.files))assert.equal(sha(fs.readFileSync(path.join(dir,file))),hash,'Bad seal '+dir+'/'+file);
};
async function timeline(file){
  const gzip=createGunzip({finishFlush:constants.Z_SYNC_FLUSH}),input=fs.createReadStream(file);input.pipe(gzip);
  const lines=readline.createInterface({input:gzip}),stages={},summary={records:0,stages};
  for await(const line of lines){
    let entry;try{entry=JSON.parse(line);}catch{summary.truncated_last_record=true;break;}
    summary.records++;
    const row=entry.cells[0].map(cell=>cell.symbol).join('');
    const s=stages[entry.stage]??={first_at_ns:entry.source_at_ns,last_at_ns:entry.source_at_ns,rows:[],records:0,frame_hashes:new Set()};
    s.last_at_ns=entry.source_at_ns;s.records++;s.frame_hashes.add(entry.frame_sha256);
    if(s.rows.at(-1)?.row!==row)s.rows.push({at_ns:entry.source_at_ns,raw_byte_end:entry.raw_byte_end,row,glyphs:[...row].filter(c=>'⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'.includes(c))});
  }
  for(const s of Object.values(stages)){s.changed_frames=s.frame_hashes.size;delete s.frame_hashes;}
  return summary;
}
if(args.diagnostic){
  const dir=path.resolve(args.diagnostic);report.diagnostic={directory:dir,sealed:false,qualification:'Interrupted/diagnostic input is never acceptance evidence',timeline:await timeline(path.join(dir,'timeline.jsonl.gz'))};
  report.capture_gate='BLOCKED_INCOMPLETE_ATTEMPT';
}else{
  for(const [side,argument]of [['reference',args.reference],['native',args.native]]){
    if(!argument)continue;
    const entry=report.sides[side]={directories:[],binaries:[],attempts:[],captures:[]};
    for(const argumentRoot of argument.split(',')){
    const dir=path.resolve(argumentRoot);seal(dir);
    const lock=JSON.parse(fs.readFileSync(path.join(dir,'capture.lock.json')));
    if(entry.configured_layout)assert.equal(entry.configured_layout,lock.tabs);
    if(entry.indicator_mode)assert.equal(entry.indicator_mode,lock.indicators);
    entry.configured_layout=lock.tabs;entry.indicator_mode=lock.indicators;
    entry.directories.push({directory:dir,lock_sha256:sha(fs.readFileSync(path.join(dir,'capture.lock.json'))),campaign_complete:lock.campaign_complete??lock.attempts.length===lock.profiles.length*lock.animations.length*lock.binaries.length});
    entry.binaries.push(...lock.binaries);
    for(const attempt of lock.attempts){
      const local=path.join(dir,attempt.name);seal(local);assert.equal(sha(fs.readFileSync(path.join(local,'seal.json'))),attempt.seal_sha256);
      const checks=JSON.parse(fs.readFileSync(path.join(local,'checks.json'))),protocol=JSON.parse(fs.readFileSync(path.join(local,'protocol.json')));
      const captures=lock.captures.filter(c=>c.name===attempt.name);
      for(const capture of captures){
        for(const [extension,hash]of Object.entries(capture.files))assert.equal(sha(fs.readFileSync(path.join(local,capture.label+'.'+extension))),hash);
        const cells=JSON.parse(fs.readFileSync(path.join(local,capture.label+'.cells.json'))),render=JSON.parse(fs.readFileSync(path.join(local,capture.label+'.render.json'))),png=fs.readFileSync(path.join(local,capture.label+'.png'));
        assert.equal(cells.columns,capture.columns);assert.equal(cells.rows,capture.rows);assert.equal(cells.cells.length,cells.rows);assert(cells.cells.every(row=>row.length===cells.columns));
        assert.deepEqual(cells.cursor,render.cursor);assert.equal(png.readUInt32BE(16),render.png_width);assert.equal(png.readUInt32BE(20),render.png_height);
        assert.equal(Math.ceil(render.before.screen_rect.width),render.png_width);assert.equal(Math.ceil(render.before.screen_rect.height),render.png_height);
        assert.equal(render.product_paused,false);assert.equal(render.application_clock_replaced,false);assert.equal(render.unmasked,true);
        if(render.title_observation){const {line,start,width,text}=render.title_observation;assert(cells.cells[line].slice(start,start+width).map(cell=>cell.symbol).join('').includes(text),'Attested title phase absent');}
        if(render.observed_glyph)assert(cells.cells[render.glyph_line??0].some(cell=>cell.symbol===render.observed_glyph),'Attested glyph absent');
        const tabGlyphs=cells.cells.flatMap((row,y)=>row.flatMap((cell,x)=>(attempt.name.includes('-vertical-')?x<5:y===0)&&'⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'.includes(cell.symbol)&&cell.symbol.trim()?[{x,y,symbol:cell.symbol}]:[]));
        if(render.tab_spinner_glyphs)assert.deepEqual(render.tab_spinner_glyphs,tabGlyphs);
        if(capture.tab_spinner_glyphs)assert.deepEqual(capture.tab_spinner_glyphs,tabGlyphs);
      }
      entry.captures.push(...captures.map(c=>({...c,directory:dir})));
      const observations=await timeline(path.join(local,'timeline.jsonl.gz'));
      const functional=checks.status==='PASS_BOUNDED_OWNER_AND_OBSERVATIONS'&&attempt.counts.requests===6&&attempt.counts.completed===5&&attempt.counts.invalid===0&&protocol.some(e=>e.kind==='exit'&&e.code===0&&e.termination==='natural');
      const temporal=checks.hover.every(h=>h.exactly_one_pointer_input&&h.owner_rows_unchanged&&h.requests_unchanged&&h.project_unchanged&&h.checks_ok&&(!h.overflow||h.captures.some(c=>c.offset===1)));
      const settled=checks.idle.filter(i=>!i.label.startsWith('busy')).every(i=>i.no_input&&i.styled_grid_and_cursor_equal&&i.terminal_bytes===0);
      const actualSpinner=new Set(checks.spinner?.observations.flatMap(o=>o.glyphs)||[]);
      const spinner=lock.indicators==='numbers'?checks.spinner?.observations.every(o=>o.glyphs.length===0):checks.spinner?.animations?actualSpinner.size>1:actualSpinner.size===1&&actualSpinner.has('⠋');
      const item={name:attempt.name,directory:dir,status:attempt.status,functional,temporal,settled,spinner,observed_spinner_glyphs:[...actualSpinner],captures:captures.length,
        missing_first_offset:checks.hover.filter(h=>h.overflow&&!h.captures.some(c=>c.offset===1)).map(h=>h.kind),
        hover:checks.hover.map(({samples,...h})=>({...h,samples:samples.length})),idle:checks.idle,timeline:observations};
      if(fs.existsSync(path.join(local,'native-ui-metrics.json')))item.native_metrics=JSON.parse(fs.readFileSync(path.join(local,'native-ui-metrics.json')));
      entry.attempts.push(item);
      if(!functional||!temporal||!settled||!spinner)report.failed_attempt_history.push({side,directory:dir,name:attempt.name,functional,temporal,settled,spinner});
    }
    if(side==='native'){assert(lock.binaries.some(b=>b.origin==='oc'&&b.parent_release));assert.equal(lock.native_source_changed,false,'Native source changed during capture');}
    }
    const current=new Map(entry.attempts.map(attempt=>[attempt.name,attempt]));
    entry.selected_attempts=[...current.values()].map(attempt=>({name:attempt.name,directory:attempt.directory}));
    entry.captures=entry.captures.filter(c=>current.get(c.name)?.directory===c.directory);
    for(const item of current.values())if(!item.functional||!item.temporal||!item.settled||!item.spinner)report.failures.push({side,directory:item.directory,name:item.name,functional:item.functional,temporal:item.temporal,settled:item.settled,spinner:item.spinner});
    const expected=['80x24','120x40','160x48'].flatMap(size=>['on','off'].map(animation=>`${side==='reference'?'upstream':'oc'}-${size}-${animation}-horizontal-status`));
    const qualified=name=>{const attempt=current.get(name);return attempt&&attempt.functional&&attempt.temporal&&attempt.settled&&attempt.spinner&&['long','unicode','short','exact','restfit','inactive','busy'].every(kind=>attempt.hover.some(h=>h.kind===kind));};
    entry.full_horizontal_matrix=expected.every(qualified);
    entry.full_configured_matrix=['80x24','120x40','160x48'].flatMap(size=>['on','off'].map(animation=>`${side==='reference'?'upstream':'oc'}-${size}-${animation}-${entry.configured_layout}-${entry.indicator_mode}`)).every(qualified);
    const idle=[...current.values()].flatMap(a=>a.idle).filter(i=>!i.label.startsWith('busy'));
    const firstOffsets=[...current.values()].flatMap(a=>a.hover.flatMap(h=>h.captures.filter(c=>c.offset===1).map(c=>c.source_elapsed_ms))).filter(Number.isFinite);
    entry.summary={selected_attempts:current.size,full_horizontal_matrix:entry.full_horizontal_matrix,full_configured_matrix:entry.full_configured_matrix,captures:entry.captures.length,settled_idle_windows:idle.length,
      all_idle_windows_no_input:idle.every(i=>i.no_input),all_idle_windows_full_styled_grid_cursor_equal:idle.every(i=>i.styled_grid_and_cursor_equal),all_idle_windows_zero_pty_bytes:idle.every(i=>i.terminal_bytes===0),
      actual_product_idle_cpu_percent_range:idle.length?[Math.min(...idle.map(i=>i.cpu_percent)),Math.max(...idle.map(i=>i.cpu_percent))]:null,
      observed_first_offset_source_ms_range:firstOffsets.length?[Math.min(...firstOffsets),Math.max(...firstOffsets)]:null,
      idle_counter_qualification:'CPU/context switches are measured on actual product process/all threads. Zero PTY bytes does not attest zero application redraws or scheduler wakes. Native app-counter window gate remains explicit.'};
  }
  if(report.sides.reference&&report.sides.native){
    const left=report.sides.reference,right=report.sides.native;
    for(const capture of left.captures){
      const rightName=capture.name.replace(/^upstream-/,'oc-');
      const other=right.captures.find(c=>c.name===rightName&&c.label===capture.label);
      const pair={name:capture.name,label:capture.label,phase_match:false};report.comparisons.push(pair);
      if(!other){pair.status='BLOCKED_MISSING_MATCHED_FRAME';continue;}
      // Match genuinely observed component phases; a common elapsed clock is not
      // enough and arbitrary current frames are not silently considered aligned.
      const actualGlyphs=c=>c.tab_spinner_glyphs??(()=>{
        const cells=JSON.parse(fs.readFileSync(path.join(c.directory,c.name,c.label+'.cells.json')));
        return cells.cells.flatMap((row,y)=>row.flatMap((cell,x)=>(c.name.includes('-vertical-')?x<5:y===0)&&'⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'.includes(cell.symbol)&&cell.symbol.trim()?[{x,y,symbol:cell.symbol}]:[]));
      })();
      pair.reference_observed_tab_glyphs=actualGlyphs(capture);pair.native_observed_tab_glyphs=actualGlyphs(other);
      pair.title_phase_match=capture.observed_offset===other.observed_offset&&capture.expected_offset===other.expected_offset&&capture.title===other.title;
      pair.tab_glyph_phase_match=capture.observed_glyph===other.observed_glyph&&JSON.stringify(pair.reference_observed_tab_glyphs.map(g=>g.symbol))===JSON.stringify(pair.native_observed_tab_glyphs.map(g=>g.symbol));
      pair.phase_match=pair.title_phase_match&&pair.tab_glyph_phase_match;
      if(!pair.title_phase_match){pair.status='BLOCKED_DIFFERENT_OBSERVED_TITLE_PHASE';continue;}
      // Corresponding observed title offsets are comparable even when another
      // independently mounted component has a different actual glyph phase.
      // Compare the FULL unchanged frame, retain that mismatch explicitly, and
      // never manufacture a common animation epoch or mask the other component.
      for(const kind of ['grid','png']){
        const extension=kind==='grid'?'cells.json':'png';
        const command=['/usr/bin/python3',path.join(repo,'tui-recovery/scripts/compare_frames.py'),kind,path.join(capture.directory,capture.name,capture.label+'.'+extension),path.join(other.directory,rightName,other.label+'.'+extension)];
        const compared=spawnSync(command[0],command.slice(1),{encoding:'utf8'});
        pair[kind]={exit_code:compared.status,...JSON.parse(compared.stdout)};
      }
      pair.status=!pair.tab_glyph_phase_match?'DIFFERENT_INDEPENDENT_TAB_GLYPH_PHASE':pair.grid.exit_code===0&&pair.png.exit_code===0?'EQUAL':'DIFFERENT_OR_BLOCKED';
    }
    report.capture_gate=report.comparisons.length&&report.comparisons.every(c=>c.status==='EQUAL')&&!report.failures.length?(left.full_horizontal_matrix&&right.full_horizontal_matrix?'EQUAL_HORIZONTAL_MATRIX_NATIVE_IDLE_COUNTER_GATE_PENDING':'EQUAL_CAPTURED_SUBSET_SCOPE_MATRIX_PENDING'):'OPEN_DIFFERENCES_OR_MISSING_PHASES';
  }
}
fs.writeFileSync(output,JSON.stringify(report,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({output,capture_gate:report.capture_gate,failures:report.failures,diagnostic_records:report.diagnostic?.timeline.records}));
if(report.failures.length||report.capture_gate==='BLOCKED_INCOMPLETE_ATTEMPT')process.exitCode=1;
