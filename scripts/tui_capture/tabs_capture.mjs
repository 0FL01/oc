#!/usr/bin/env node
// VIS41 / VIS39 own-running: real PTY, original public UI owner, no renderer injection.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {spawn,spawnSync} from 'node:child_process';
import readline from 'node:readline';
import {createGzip} from 'node:zlib';
import {fileURLToPath} from 'node:url';

const here=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(here,'../..');
const args={};
for(let i=2;i<process.argv.length;i+=2){assert(process.argv[i].startsWith('--')&&process.argv[i+1]);args[process.argv[i].slice(2)]=process.argv[i+1];}
for(const key of Object.keys(args))assert(['output','oracle','reference','oc','released-sha256','released-head','side','columns','rows','animations','tabs','indicators','cases','bun','timeline','reference-projects','compact-probe'].includes(key),'Unknown option '+key);
const output=path.resolve(args.output||''),oracle=path.resolve(args.oracle||'');
assert.equal(path.dirname(output),path.join(repo,'evidence/tui/recovery-v00'));
assert.match(path.basename(output),/^tabs-(reference|native|paired)[a-z0-9-]+$/);
const sha=value=>createHash('sha256').update(value).digest('hex');
const save=(dir,name,value)=>fs.writeFileSync(path.join(dir,name),JSON.stringify(value,null,2)+'\n',{flag:'wx'});
const side=args.side||'reference',tabs=args.tabs||'horizontal',indicators=args.indicators||'status';
assert(['reference','native','paired'].includes(side));assert(['horizontal','vertical'].includes(tabs));assert(['status','numbers'].includes(indicators));
assert(!args['compact-probe']||args['compact-probe']==='on'&&tabs==='vertical','Compact probe is an explicit vertical diagnostic');
const profiles=args.columns?[[Number(args.columns),Number(args.rows)]]:[[80,24],[120,40],[160,48]];
for(const [c,r]of profiles)assert([[80,24],[120,40],[160,48]].some(([a,b])=>a===c&&b===r),'Use approved profiles');
assert(!args.rows||args.columns);
assert(!args.animations||['on','off','both'].includes(args.animations));
const animations=args.animations==='on'?[true]:args.animations==='off'?[false]:[true,false];
const timeline=args.timeline||'captures';assert(['captures','all'].includes(timeline));
const cases=(args.cases||'long,unicode,short,exact,restfit').split(',');
assert(new Set(cases).size===cases.length&&cases.every(c=>['long','unicode','short','exact','restfit'].includes(c)));
const result=JSON.parse(fs.readFileSync(path.join(oracle,'result.json')));
assert.equal(result.status,'PASS_PINNED_SOURCE_ORACLE');
const oracleManifest=JSON.parse(fs.readFileSync(path.join(oracle,'source-manifest.json')));
const tools='/home/opencode/.cache/opencode-tmp/opencode/t44-reference';
const bun=args.bun||'/home/opencode/.bun/bin/bun';
const sources=()=>{
  const files=spawnSync('git',['ls-files','crates','Cargo.toml','Cargo.lock','rust-toolchain.toml'],{cwd:repo,encoding:'utf8'}).stdout.trim().split('\n').filter(Boolean);
  return Object.fromEntries(files.map(file=>[file,sha(fs.readFileSync(path.join(repo,file)))]));
};
const head=spawnSync('git',['rev-parse','HEAD'],{cwd:repo,encoding:'utf8'}).stdout.trim(),nativeSources=sources();
const binaries=[];
if(side!=='native'){
  assert(args.reference&&path.isAbsolute(args.reference),'Explicit pinned original required');
  assert.equal(sha(fs.readFileSync(args.reference)),'2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a');
  binaries.push({origin:'upstream',binary:args.reference,hash:sha(fs.readFileSync(args.reference)),commit:'2670273ff17da96f85c5826ced57aa1b368754fa'});
}
if(side!=='reference'){
  assert(args.oc&&path.isAbsolute(args.oc)&&args['released-sha256']&&args['released-head'],'Parent release hash and HEAD required before native launch');
  assert.equal(args['released-head'],head,'Release HEAD differs');
  assert.equal(sha(fs.readFileSync(args.oc)),args['released-sha256'],'Released executable changed');
  binaries.push({origin:'oc',binary:args.oc,hash:args['released-sha256'],commit:head,parent_release:true});
}
const referenceProjects=new Map();
if(args['reference-projects']){
  assert.notEqual(side,'paired','Existing sealed reference projects are for reference/native capture');
  for(const root of args['reference-projects'].split(',').map(p=>path.resolve(p))){
    const seal=JSON.parse(fs.readFileSync(path.join(root,'seal.json'))),raw=fs.readFileSync(path.join(root,'capture.lock.json'));
    assert.equal(sha(raw),seal.files['capture.lock.json']);
    const referenceLock=JSON.parse(raw);
    for(const attempt of referenceLock.attempts.filter(a=>a.name.startsWith('upstream-'))){
      const dir=path.join(root,attempt.name),sealed=fs.readFileSync(path.join(dir,'seal.json')),specification=fs.readFileSync(path.join(dir,'bridge-spec.json'));
      assert.equal(sha(sealed),attempt.seal_sha256);assert.equal(sha(specification),JSON.parse(sealed).files['bridge-spec.json']);
      const original=JSON.parse(specification),project=original.project||path.join(original.root,'project');
      assert(project.startsWith(path.join(tools,'runs')+'/')&&path.basename(project)==='project'&&fs.readdirSync(project).length===0,'Reference fixture project changed');
      referenceProjects.set(side==='reference'?attempt.name:attempt.name.replace(/^upstream-/,'oc-'),{project,reference_root:root,reference_attempt:attempt.name,reference_status:attempt.status});
    }
  }
}
// Existing attempts are immutable. All fixture data is outside the repository.
fs.mkdirSync(output);
const browserHome=path.join(tools,'runs',path.basename(output)+'-browser');fs.mkdirSync(browserHome);
const env={HOME:browserHome,PATH:'/usr/bin:/bin',LANG:'C.UTF-8',LC_ALL:'C.UTF-8',TZ:'UTC',PLAYWRIGHT_BROWSERS_PATH:path.join(tools,'browsers')};
process.env.PLAYWRIGHT_BROWSERS_PATH=env.PLAYWRIGHT_BROWSERS_PATH;
const require=createRequire(path.join(tools,'package.json')),{chromium}=require('playwright');
fs.copyFileSync(path.join(tools,'package-lock.json'),path.join(output,'tooling.package-lock.json'));
const lock={schema_version:1,scope:'T44 VIS41 / VIS39 own-running only',native_capture_gate:side==='reference'?'PENDING_PARENT_FRESH_BINARY_RELEASE':'PENDING_FULL_PAIRED_AUDIT',source_HEAD:head,
  native_source_manifest_before:nativeSources,source_oracle:oracleManifest,source_oracle_path:oracle,
  runner_hashes:Object.fromEntries(['tabs_capture.mjs','tabs_bridge.py','tabs_oracle.ts','frontend.js'].map(file=>[file,sha(fs.readFileSync(path.join(here,file)))])),
  binaries,profiles,animations,tabs,indicators,cases,timeline,reference_projects:Object.fromEntries(referenceProjects),commands:process.argv,planned_attempts:profiles.length*animations.length*binaries.length,attempts:[],captures:[],
  animation_clock:'Real independently mounted components only. No global synchronization, SIGSTOP, application time replacement, masks, crops, or tolerance.',
  screenshot_method:'Hold only frontend VT ingestion at an observed byte boundary; product remains live. Full styled grid/cursor and full-terminal PNG from that same real VT prefix. Deferred real output is retained and applied after the PNG.',
  compact:'NOT_QUALIFIED: no compact-only renderer branch is invented. Vertical captures must be checked against the real released native branch.'};
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
let browser;
try{
  browser=await chromium.launch({headless:true,env});
  lock.frontend={chromium:browser.version(),playwright:require('playwright/package.json').version,xterm:require('@xterm/xterm/package.json').version,unicode:require('@xterm/addon-unicode11/package.json').version,font:'DejaVu Sans Mono',font_size:14,device_scale_factor:1,padding:0,unmasked:true};
  for(const [columns,rows]of profiles)for(const animated of animations)for(const binary of binaries){
    const name=`${binary.origin}-${columns}x${rows}-${animated?'on':'off'}-${tabs}-${indicators}`,dir=path.join(output,name);fs.mkdirSync(dir);
    const spec={origin:binary.origin,binary:binary.binary,binary_sha256:binary.hash,columns,rows,animations:animated,tabs,indicators,
      root:path.join(tools,'runs',path.basename(output)+'-'+name),evidence:dir};
    if(args['reference-projects']){assert(referenceProjects.has(name),'Missing sealed original project for '+name);spec.project=referenceProjects.get(name).project;spec.reference_project=referenceProjects.get(name);}
    if(side==='paired')spec.project=path.join(tools,'runs',path.basename(output)+`-${columns}x${rows}-${animated?'on':'off'}-${tabs}-${indicators}-shared`,'project');
    save(dir,'bridge-spec.json',spec);
    const page=await browser.newPage({viewport:{width:1800,height:1100},deviceScaleFactor:1});
    await page.setContent('<style>html,body{margin:0;background:#0a0a0a}#terminal{display:inline-block;font-variant-ligatures:none}.xterm-viewport{scrollbar-width:none}</style><div id="terminal"></div>');
    for(const file of ['node_modules/@xterm/xterm/css/xterm.css'])await page.addStyleTag({path:path.join(tools,file)});
    for(const file of ['node_modules/@xterm/xterm/lib/xterm.js','node_modules/@xterm/addon-unicode11/lib/addon-unicode11.js'])await page.addScriptTag({path:path.join(tools,file)});
    await page.addScriptTag({path:path.join(here,'frontend.js')});
    const child=spawn('/usr/bin/python3',[path.join(here,'tabs_bridge.py'),path.join(dir,'bridge-spec.json')],{env,stdio:['pipe','pipe','pipe']});
    const closed=new Promise(resolve=>child.once('close',resolve));
    const logs=[],inputs=[],chunks=[],chunkEvents=[],observations=[],deferred=[],pendingOutput=[];
    const gzip=createGzip(),timelineFile=fs.createWriteStream(path.join(dir,'timeline.jsonl.gz'),{flags:'wx'});gzip.pipe(timelineFile);
    const timelineClosed=new Promise(resolve=>timelineFile.once('close',resolve));
    let queue=Promise.resolve(),holdDepth=0,applied=0,exit,stderr='',error,stage='startup',lastFrame,watcher,draining=false,watcherMarkers=[];
    child.once('exit',code=>{exit=code;});child.stderr.on('data',data=>{stderr+=data;fs.appendFileSync(path.join(dir,'bridge.stderr.live.txt'),data);});
    child.stdin.on('error',e=>{error=e;});
    const control=value=>{if(!child.stdin.destroyed)child.stdin.write(JSON.stringify(value)+'\n');};
    const send=(text,label='terminal_reply')=>{const data=Buffer.from(text).toString('base64'),event={at_ns:process.hrtime.bigint().toString(),label,base64:data};inputs.push(event);fs.appendFileSync(path.join(dir,'inputs.live.jsonl'),JSON.stringify(event)+'\n');control({kind:'input',data,label});};
    await page.exposeFunction('terminalReply',value=>send(value));await page.evaluate(p=>startTerminal(p),{columns,rows});
    await page.evaluate(()=>{
      window.readFastTerminal=()=>{
        const buffer=term.buffer.active;
        const symbols=Array.from({length:term.rows},(_,y)=>Array.from({length:term.cols},(_,x)=>{
          const cell=buffer.getLine(buffer.viewportY+y).getCell(x);return cell.getChars()||(cell.getWidth()===0?'':' ');
        }));
        return {columns:term.cols,rows:term.rows,symbols,cursor:{x:buffer.cursorX,y:buffer.cursorY,visible:!term._core.coreService.isCursorHidden,shape:term._core.coreService.decPrivateModes.cursorStyle||term.options.cursorStyle}};
      };
    });
    const fast=async()=>{
      const f=await page.evaluate(()=>readFastTerminal());return {...f,cells:f.symbols.map(row=>row.map(symbol=>({symbol}))),text:f.symbols.map(row=>row.join('')).join('\n')};
    };
    const pump=()=>{
      if(draining||holdDepth||!pendingOutput.length)return;
      draining=true;
      queue=(async()=>{while(pendingOutput.length&&!holdDepth){
        let size=pendingOutput.length;
        const candidates=[];
        // Candidate text only chooses an actual VT byte boundary. The real
        // parsed grid subsequently validates every attested phase/glyph.
        for(let i=0;i<pendingOutput.length;i++){
          candidates.push(Buffer.from(pendingOutput[i].data,'base64'));
          const candidate=Buffer.concat(candidates).toString('utf8').replace(/\x1b\[[0-?]*[ -/]*[@-~]/g,'').replace(/\x1b\][^\x07]*(?:\x07|\x1b\\)/g,'');
          if(watcherMarkers.some(text=>candidate.includes(text))){size=i+1;break;}
        }
        const batch=pendingOutput.splice(0,size),event=batch.at(-1),data=Buffer.concat(batch.map(e=>Buffer.from(e.data,'base64'))).toString('base64');
        const lightweight=await page.evaluate(async data=>{await writeTerminal(data);return readFastTerminal();},data);applied=event.byte_end;
        const f={...lightweight,cells:lightweight.symbols.map(row=>row.map(symbol=>({symbol}))),text:lightweight.symbols.map(row=>row.join('')).join('\n')};lastFrame=f;
        event.stage=stage;event.frontend_at_ns=process.hrtime.bigint().toString();event.columns=f.columns;event.rows=f.rows;
        for(const original of batch){const metadata=chunkEvents.find(e=>e.byte_end===original.byte_end);Object.assign(metadata,{stage,frontend_at_ns:event.frontend_at_ns,columns:f.columns,rows:f.rows,coalesced_to_byte_end:applied});}
        observations.push({source_at_ns:event.at_ns,frontend_at_ns:event.frontend_at_ns,raw_byte_end:applied,stage,
          symbols:f.symbols.map(row=>row.join('')),cursor:f.cursor,frame_sha256:sha(JSON.stringify(f))});
        if(watcher)await watcher(f,event);
      }})().catch(e=>{error=e;}).finally(()=>{draining=false;pump();});
    };
    const enqueue=event=>{pendingOutput.push(event);pump();};
    readline.createInterface({input:child.stdout}).on('line',line=>{
      const event=JSON.parse(line);
      if(event.kind!=='output'){logs.push(event);fs.appendFileSync(path.join(dir,'protocol.live.jsonl'),JSON.stringify(event)+'\n');return;}
      const bytes=Buffer.from(event.data,'base64');chunks.push(bytes);event.byte_end=(chunkEvents.at(-1)?.byte_end||0)+bytes.length;
      chunkEvents.push({at_ns:event.at_ns,bytes:bytes.length,byte_end:event.byte_end,sha256:sha(bytes)});fs.appendFileSync(path.join(dir,'raw.vt'),bytes);
      if(holdDepth)deferred.push(event);else enqueue(event);
    });
    const frame=async()=>{if(error)throw error;return lastFrame||fast();};
    const fullFrame=async()=>{
      holdDepth++;try{await queue;if(error)throw error;return await page.evaluate(()=>readTerminal());}
      finally{holdDepth--;if(!holdDepth){for(const event of deferred.splice(0))pendingOutput.push(event);pump();}}
    };
    const wait=async(predicate,label,ms=15000)=>{
      const end=performance.now()+ms;
      while(performance.now()<end){const f=await frame();if(predicate(f))return f;if(exit!==undefined)throw Error('Bridge exited '+exit+' waiting '+label+' '+stderr);await sleep(20);}
      throw Error('Timeout '+label);
    };
    const waitSignal=async(predicate,label,ms=15000)=>{
      const end=performance.now()+ms;while(!predicate()){if(error)throw error;if(exit!==undefined)throw Error('Bridge exited waiting '+label);if(performance.now()>=end)throw Error('Timeout '+label);await sleep(20);}
    };
    const counts=()=>({requests:logs.filter(e=>e.kind==='provider').length,completed:logs.filter(e=>e.kind==='provider_completed').length,invalid:logs.filter(e=>e.kind==='provider'&&!e.valid).length});
    const ask=async(kind,id)=>{control({kind,request_id:id});await waitSignal(()=>logs.some(e=>e.kind===kind&&e.request_id===id),kind+' '+id,8000);return logs.find(e=>e.kind===kind&&e.request_id===id);};
    const shot=async(label,extra={},atBoundary=false)=>{
      holdDepth++;const holdStart=process.hrtime.bigint().toString();
      try{
        if(!atBoundary)await queue;if(error)throw error;
        const f=await page.evaluate(()=>readTerminal());
        if(extra.title_observation){
          const {line,start,width,text}=extra.title_observation;
          assert(f.cells[line].slice(start,start+width).map(cell=>cell.symbol).join('').includes(text),'Captured title did not have the attested observed source phase');
        }
        if(extra.observed_glyph)assert(f.cells[extra.glyph_line].some(cell=>cell.symbol===extra.observed_glyph),'Captured tab did not have the attested actual glyph');
        const tabGlyphs=f.cells.flatMap((row,y)=>row.flatMap((cell,x)=>(tabs==='horizontal'?y===0:x<5)&&'⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'.includes(cell.symbol)&&cell.symbol.trim()?[{x,y,symbol:cell.symbol}]:[]));
        const busyGlyphs=extra.busy?f.cells[extra.title_observation?.line??(tabs==='horizontal'?0:railRow(f))].filter(cell=>'⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'.includes(cell.symbol)&&cell.symbol.trim()).map(cell=>cell.symbol):[];
        await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
        const before=await page.evaluate(()=>readCaptureGeometry()),rect=before.screen_rect;
        const png=await page.screenshot({path:path.join(dir,label+'.png'),clip:{x:rect.x,y:rect.y,width:Math.ceil(rect.width),height:Math.ceil(rect.height)}});
        assert.equal(sha(JSON.stringify(await page.evaluate(()=>readTerminal()))),sha(JSON.stringify(f)),'Held VT grid changed during screenshot');
        const {text,...grid}=f;
        const profile={columns:f.columns,rows:f.rows,animations:animated,tabs,indicators,...lock.frontend};
        save(dir,label+'.cells.json',{schema_version:1,origin:binary.origin,scenario:`vis41-${f.columns}x${f.rows}-${animated}-${tabs}-${indicators}-${label}`,
          producer_commit:binary.commit,fixture_sha256:sha(JSON.stringify({protocol:'six-bounded-responses',oracle_sources:oracleManifest.source_sha256,cases})),environment_id:sha(JSON.stringify(profile)),...grid});
        fs.writeFileSync(path.join(dir,label+'.txt'),text+'\n',{flag:'wx'});
        fs.writeFileSync(path.join(dir,label+'.vt'),Buffer.concat(chunks).subarray(0,applied),{flag:'wx'});
        const render={before,after:await page.evaluate(()=>readCaptureGeometry()),png_width:png.readUInt32BE(16),png_height:png.readUInt32BE(20),unmasked:true,
          cursor:f.cursor,raw_byte_end:applied,frontend_hold_start_ns:holdStart,frontend_hold_end_ns:process.hrtime.bigint().toString(),deferred_chunks:deferred.length,
          product_paused:false,application_clock_replaced:false,...extra,busy_glyphs:busyGlyphs,tab_spinner_glyphs:tabGlyphs};save(dir,label+'.render.json',render);
        lock.captures.push({name,label,columns:f.columns,rows:f.rows,...extra,busy_glyphs:busyGlyphs,tab_spinner_glyphs:tabGlyphs,files:Object.fromEntries(['cells.json','png','txt','vt','render.json'].map(ext=>[ext,sha(fs.readFileSync(path.join(dir,label+'.'+ext)))]))});
        if(timeline==='captures')gzip.write(JSON.stringify({source_at_ns:chunkEvents.find(event=>event.byte_end===applied)?.at_ns??null,
          frontend_at_ns:render.frontend_hold_end_ns,raw_byte_end:applied,stage,label,frame_sha256:sha(JSON.stringify(f)),decode:'actual full-grid screenshot boundary; complete all-chunk VT is separately retained',...f})+'\n');
        return f;
      }finally{holdDepth--;if(!holdDepth){for(const event of deferred.splice(0))pendingOutput.push(event);pump();}}
    };
    const report={name,status:'IN_PROGRESS',hover:[],idle:[],checks:[],counts:[],observed_phase_matching:'Source-title text and actual glyph observed independently, never elapsed global clock'};
    const checkpoint=()=>fs.writeFileSync(path.join(dir,'checks.live.json'),JSON.stringify(report,null,2)+'\n');
    const rootRows=snapshot=>snapshot.observations.filter(o=>['session','session_v2','sessions'].includes(o.table)).flatMap(o=>o.rows);
    const roots=snapshot=>rootRows(snapshot).filter(row=>!row.parent_id&&!row.parentID);
    const railRow=f=>tabs==='horizontal'?0:f.cells.findIndex(row=>row.slice(0,42).map(c=>c.symbol).join('').includes('VIS41'));
    const tabTarget=(f,title)=>{
      for(let y=0;y<f.rows;y++){
        if(tabs==='horizontal'&&y!==0)break;
        const cells=f.cells[y],text=cells.map(c=>c.symbol).join(''),p=text.indexOf(title.slice(0,10));
        if(p>=0){const x=cells.findIndex((_,i)=>cells.slice(0,i).map(c=>c.symbol).join('').length===p);return {x:x+2,y:y+1};}
      }
      throw Error('No painted title target '+title);
    };
    const move=(x,y,label)=>send(`\x1b[<35;${x};${y}M`,label);
    const leave=()=>move((lastFrame?.columns??columns)-2,Math.max(3,(lastFrame?.rows??rows)-10),'pointer_leave');
    const click=target=>{send(`\x1b[<0;${target.x};${target.y}M`,'tab_select_down');send(`\x1b[<0;${target.x};${target.y}m`,'tab_select_up');};
    const titleInfo=(title,width)=>{
      const found=spawnSync(bun,[path.join(here,'tabs_oracle.ts'),oracle,'phases',title,String(width)],{cwd:repo,encoding:'utf8'});
      assert.equal(found.status,0,found.stderr);return JSON.parse(found.stdout);
    };
    const rename=async(title,label)=>{
      leave();send('\x12','rename_ctrl_r');await wait(f=>f.text.includes('Rename session'),'real rename dialog');
      send('\x1b[H','rename_home');await sleep(80);send('\x1b[1;2F','rename_shift_end');await sleep(80);send('\x1b[200~'+title+'\x1b[201~','rename_title_paste');await sleep(100);
      await shot(label+'-rename-input',{requested_title:title});send('\r','rename_commit');await wait(f=>!f.text.includes('Rename session'),'rename dismissed');
      await sleep(animated?500:150);
      const owner=await ask('owner_snapshot',label+'-rename-owner');assert(roots(owner).some(row=>row.title===title),'Exact title not committed by real owner');
      report.checks.push({label,requested_title:title,owner_rows:roots(owner),counts:counts()});return owner;
    };
    const idle=async(label,ms=1500)=>{
      stage=label;const before=await fullFrame(),start=await ask('process_sample',label+'-start'),inputStart=logs.filter(e=>e.kind==='input_written').length;
      await sleep(ms);const end=await ask('process_sample',label+'-end'),after=await fullFrame();
      const writes=chunkEvents.filter(e=>e.at_ns>=start.at_ns&&e.at_ns<=end.at_ns),sameThreads=JSON.stringify(start.threads.map(t=>t.tid))===JSON.stringify(end.threads.map(t=>t.tid));
      const observation={label,start,end,duration_ms:(end.at_ns-start.at_ns)/1e6,cpu_ticks:end.cpu_ticks-start.cpu_ticks,cpu_ms:1000*(end.cpu_ticks-start.cpu_ticks)/end.clock_ticks_per_second,
        cpu_percent:100000*(end.cpu_ticks-start.cpu_ticks)/end.clock_ticks_per_second/((end.at_ns-start.at_ns)/1e6),terminal_chunks:writes.length,terminal_bytes:writes.reduce((n,e)=>n+e.bytes,0),
        no_input:logs.filter(e=>e.kind==='input_written').length===inputStart,styled_grid_and_cursor_equal:sha(JSON.stringify(before))===sha(JSON.stringify(after)),
        same_threads:sameThreads,voluntary_context_switches:end.voluntary_context_switches-start.voluntary_context_switches,involuntary_context_switches:end.involuntary_context_switches-start.involuntary_context_switches,
        scheduler_counters:binary.origin==='oc'?'OC_TUI_TEST_METRICS collected at natural exit; frame samples independent of PTY writes':'No original app counter API; actual product CPU/context switches and PTY writes only'};
      observation.thread_scheduler_deltas=end.threads.flatMap(thread=>{
        const first=start.threads.find(t=>t.tid===thread.tid);if(first?.scheduler_runtime_ns===undefined)return [];
        return [{tid:thread.tid,runtime_ns:thread.scheduler_runtime_ns-first.scheduler_runtime_ns,timeslices:thread.scheduler_timeslices-first.scheduler_timeslices,
          observed_interval_inside_ns:[first.sample_end_ns,thread.sample_start_ns],state_before:first.state,state_after:thread.state}];
      });
      report.idle.push(observation);checkpoint();return observation;
    };
    const hover=async(title,kind,width,{busy=false,inactive=false}={})=>{
      stage=kind;leave();await sleep(100);
      const before=await ask('owner_snapshot',kind+'-before'),f=await frame(),target=tabTarget(f,title);
      const phases=titleInfo(title,width),line=target.y-1,start=performance.now(),inputIndex=logs.filter(e=>e.kind==='input_written').length;
      const titleStart=target.x-2,textAt=f=>f.cells[line].slice(titleStart,titleStart+width).map(cell=>cell.symbol).join('');
      const samples=[],captures=[],wanted=new Set([1,2,phases.cycle_width-3]);let entered;
      const zeroPhase={line,start:titleStart,width,text:phases.phases[0].text};
      watcher=async(f,event)=>{
        const text=textAt(f),matches=phases.phases.filter(p=>p.offset>0&&text.includes(p.text)),offset=matches.length===1?matches[0].offset:null;
        const input=logs.find(e=>e.kind==='input_written'&&e.label===kind+'-enter');
        const sample={elapsed_ms:performance.now()-start,source_elapsed_ms:input?(event.at_ns-input.at_ns)/1e6:null,source_at_ns:event.at_ns,raw_byte_end:applied,text,observed_source_offsets:matches.map(p=>p.offset),cursor:f.cursor};samples.push(sample);
        if(!entered&&input&&event.at_ns>=input.at_ns&&text.includes(zeroPhase.text)&&(busy||f.cells[line].some(cell=>cell.symbol==='✕'))&&(!phases.overflow||[' ','✕'].includes(f.cells[line][titleStart+width].symbol))){
          entered=await shot(kind+'-enter',{title,hover_width:width,busy,inactive,expected_offset:0,title_observation:zeroPhase,source_at_ns:event.at_ns},true);
        }
        if(phases.overflow&&offset!==null&&wanted.has(offset)){
          wanted.delete(offset);captures.push({offset,elapsed_ms:sample.elapsed_ms,source_elapsed_ms:sample.source_elapsed_ms});
          await shot(kind+'-offset-'+offset,{title,hover_width:width,busy,inactive,observed_offset:offset,source_at_ns:event.at_ns,
            title_observation:{line,start:titleStart,width,text:matches[0].text}},true);
        }
        // VT diff renderers may preserve an unchanged repeated-letter suffix;
        // seek a distinctive prefix, then attest the complete parsed title.
        watcherMarkers=[...(!entered?[zeroPhase.text]:[]),...phases.phases.filter(p=>phases.overflow&&wanted.has(p.offset)).map(p=>p.text)].map(text=>[...text].slice(0,7).join(''));
      };
      watcherMarkers=[zeroPhase.text,...phases.phases.filter(p=>phases.overflow&&wanted.has(p.offset)).map(p=>p.text)].map(text=>[...text].slice(0,7).join(''));
      move(target.x,target.y,kind+'-enter');
      await waitSignal(()=>entered,'observed hover offset-zero '+kind,8000);
      // Title observations identify source offsets; capture/comparison is always
      // full-screen, including the real independently moving spinner.
      const base=textAt(entered);
      if(phases.overflow){
        const deadline=performance.now()+600+80*phases.cycle_width+2500;
        while(performance.now()<deadline){
          const text=textAt(lastFrame);
          if(performance.now()-start>600+80*(phases.cycle_width-1)+300&&text===base)break;
          await sleep(25);
        }
      }else await sleep(1300);
      watcher=undefined;watcherMarkers=[];
      await sleep(350);await shot(kind+'-settled',{title,hover_width:width,busy,inactive,expected_offset:0,title_observation:zeroPhase});
      const observation=await idle(kind+'-idle',busy?500:1500),after=await ask('owner_snapshot',kind+'-after');
      const changed=samples.some(sample=>sample.text!==base),returned=textAt(await frame())===base;
      const actualInputs=logs.filter(e=>e.kind==='input_written').slice(inputIndex);
      const ownerStable=JSON.stringify(before.observations)===JSON.stringify(after.observations);
      const hoverResult={kind,title,width,source_cycle_width:phases.cycle_width,overflow:phases.overflow,target,busy,inactive,samples,captures,
        first_changed_ms:samples.find(s=>s.text!==base)?.elapsed_ms??null,changed,returned_to_entered_text:returned,
        exactly_one_pointer_input:actualInputs.length===1&&actualInputs[0].label===kind+'-enter',owner_rows_unchanged:ownerStable,
        requests_unchanged:before.requests===after.requests&&before.completed===after.completed,project_unchanged:JSON.stringify(before.project_files)===JSON.stringify(after.project_files),
        idle:observation.label,checks_ok:phases.overflow?changed&&returned:!changed};
      report.hover.push(hoverResult);checkpoint();
      assert(hoverResult.exactly_one_pointer_input&&hoverResult.requests_unchanged&&hoverResult.project_unchanged,'Hover caused input/request/project effect');
      assert(ownerStable,'Hover changed committed owner rows');
      assert(hoverResult.checks_ok,'Hover motion/return observation failed '+kind);
      // A second real motion inside the same stable tab must not restart a cycle.
      move(target.x+1,target.y,kind+'-same-tab-motion');const repeatBase=textAt(await frame());await sleep(1100);
      await shot(kind+'-same-tab-no-restart',{title,hover_width:width,busy,inactive,expected_offset:0,title_observation:zeroPhase});
      assert.equal(textAt(await frame()),repeatBase,'Same-tab motion restarted marquee');
      if(!busy&&tabs==='horizontal'){
        move(titleStart+width+1,target.y,kind+'-nested-close-motion');await sleep(300);
        await shot(kind+'-nested-close',{title,hover_width:width,busy,inactive,expected_offset:0,title_observation:zeroPhase});
        assert.equal(textAt(await frame()),repeatBase,'Nested close hover restarted or reset the same tab');
      }
      leave();await sleep(150);await shot(kind+'-leave',{title,hover_width:width,busy,inactive});
    };
    try{
      await wait(f=>f.text.includes('Ask anything'),'initial Home',25000);await shot('home');
      for(const seed of ['seed-a','seed-b']){
        if(seed==='seed-b'){
          send('/new','new_root_type');await wait(f=>f.text.includes('/new'),'new command typed');send('\r','new_root_enter');await wait(f=>f.text.includes('Ask anything'),'new Home');
        }
        stage=seed;send('\x1b[200~VIS41 '+seed+': no tools or filesystem effects.\x1b[201~','seed_prompt');send('\r','seed_submit');
        await wait(f=>f.text.includes('VIS41-DONE '+seed)&&counts().completed===(seed==='seed-a'?2:4),'seed main plus title complete',25000);await sleep(1000);await shot(seed+'-completed');
      }
      let owner=await rename('Short','root-b-short');assert.equal(roots(owner).length,2,'Expected two real root sessions');
      const rootB=roots(owner).find(row=>row.title==='Short');report.root_b=rootB.id;
      const first=tabTarget(await frame(),'VIS41 fixture A');click(first);await wait(f=>f.text.includes('VIS41-DONE seed-a'),'root A selected');leave();await sleep(500);
       const oracleProfile=result.profiles.find(p=>p.columns===columns),expectedWidth=tabs==='horizontal'||columns<106?oracleProfile.horizontal_hovered[0]:37;
       const paintedFirst=tabTarget(await frame(),'VIS41 fixture A'),effectiveTabs=paintedFirst.y===1?'horizontal':'vertical';
       const observedRailWidth=effectiveTabs==='vertical'?Math.min(42,columns-44):null;
       const hoverWidth=effectiveTabs==='horizontal'?oracleProfile.horizontal_hovered[0]:observedRailWidth-5;
       report.geometry={configured_tabs:tabs,effective_tabs:effectiveTabs,reference_hover_width:expectedWidth,observed_hover_width:hoverWidth,rail_width:observedRailWidth,
         basis:'Actual first painted root title row selects horizontal/vertical. Released/pinned 42-cell rail and 44-cell content minimum give prospective width; each complete captured title independently attests that width. Exact/restfit input titles retain the reference geometry.'};
       const titles={long:'VIS41 long-title abcdefghijklmnopqrstuvwxyz 0123456789',unicode:'VIS41 Plan 🧭 日本語 é · title for the release',short:'Short-A',exact:'Exact-'+('E'.repeat(expectedWidth-6)),restfit:'Restfit-'+('R'.repeat(expectedWidth-7))};
      for(const kind of cases){
        owner=await rename(titles[kind],kind);report.root_a??=roots(owner).find(row=>row.title===titles[kind]).id;
        await shot(kind+'-rest',{title:titles[kind],hover_width:hoverWidth});await hover(titles[kind],kind,hoverWidth);
      }
      // Restore the long title and use the actual inactive tab hit target.
      await rename(titles.long,'inactive-long');click(tabTarget(await frame(),'Short'));await wait(f=>f.text.includes('VIS41-DONE seed-b'),'root B selected');leave();await sleep(500);
      await hover(titles.long,'inactive',hoverWidth,{inactive:true});
      click(tabTarget(await frame(),titles.long));await wait(f=>f.text.includes('VIS41-DONE seed-a'),'root A selected again');leave();await sleep(300);
      stage='running-complete';send('\x1b[200~VIS41 complete: no tools or filesystem effects.\x1b[201~','running_prompt');send('\r','running_submit');
      await wait(()=>logs.some(e=>e.kind==='provider_held'&&e.case==='complete'),'held actual main request');
      if(indicators==='status')await wait(f=>f.cells.some(row=>row.some(cell=>'⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'.includes(cell.symbol)&&cell.symbol.trim())),'actual spinner');
      await shot('running-before-hover');await hover(titles.long,'busy',hoverWidth,{busy:true});
      const spinnerLine=railRow(await frame());const spinnerObservations=observations.filter(o=>o.stage.startsWith('running')||o.stage.startsWith('busy')).map(o=>({at_ns:o.source_at_ns,raw_byte_end:o.raw_byte_end,row:o.symbols[spinnerLine],glyphs:[...o.symbols[spinnerLine]].filter(c=>'⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'.includes(c))}));
      report.spinner={line:spinnerLine,animations:animated,indicators,observations:spinnerObservations};
      for(const glyph of animated&&indicators==='status'?['⠋','⠙']:indicators==='status'?['⠋']:['1']){
        if(!animated||indicators==='numbers'){
          await shot('running-glyph-'+glyph.codePointAt(0).toString(16),{observed_glyph:glyph,glyph_line:spinnerLine});continue;
        }
        let captured=false;stage='running-glyph-'+glyph.codePointAt(0).toString(16);watcherMarkers=[glyph];
        watcher=async(f,event)=>{
          if(!captured&&f.cells[spinnerLine].some(cell=>cell.symbol===glyph)){
            await shot(stage,{observed_glyph:glyph,glyph_line:spinnerLine,source_at_ns:event.at_ns},true);captured=true;
          }
        };
        await waitSignal(()=>captured,'observed actual tab glyph '+glyph,6000);watcher=undefined;watcherMarkers=[];
      }
      control({kind:'release',request_id:'complete'});await wait(f=>f.text.includes('VIS41-DONE complete')&&counts().completed===5,'actual running completed');await sleep(1700);await shot('completed');
      await idle('completed-idle');await ask('owner_snapshot','completed-owner');
      stage='running-cancel';send('\x1b[200~VIS41 cancel: no tools or filesystem effects.\x1b[201~','cancel_prompt');send('\r','cancel_submit');await wait(()=>logs.some(e=>e.kind==='provider_held'&&e.case==='cancel'),'held cancellation request');
      await sleep(200);await shot('cancel-running');send('\x1b','cancel_escape_first');await sleep(100);send('\x1b','cancel_escape_second');
      await wait(f=>!f.cells[spinnerLine].some(cell=>'⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'.includes(cell.symbol)&&cell.symbol.trim()),'cancel removes own-running tab spinner',10000);
      await waitSignal(()=>logs.some(event=>event.kind==='provider_disconnected'&&event.case==='cancel'),'actual cancelled stream disconnected',10000);
      await sleep(1800);await shot('cancelled');await idle('cancelled-idle');owner=await ask('owner_snapshot','cancelled-owner');
      assert.equal(counts().requests,6);assert.equal(counts().completed,5);assert.equal(counts().invalid,0);assert.equal(roots(owner).length,2);
      assert(!owner.observations.filter(o=>['tool_ops','tool_executions'].includes(o.table)).some(o=>o.count>0),'Unexpected executed tool');
      assert.equal(Object.keys(owner.project_files).length,0,'Fixture project mutated');
       report.counts.push(counts());report.final_owner=owner;
       if(args['compact-probe']==='on'){
         assert(columns>=106,'Public original compact probe starts at an actually vertical viewport');
         stage='compact-probe';leave();await sleep(100);const before=await ask('owner_snapshot','compact-before');
         if(binary.origin==='upstream'){
           send('\x1b[<0;42;1M','compact-real-rail-down');send('\x1b[<32;5;1M','compact-real-rail-drag');send('\x1b[<0;5;1m','compact-real-rail-up');
         }else{
           control({kind:'resize',columns:50,rows:24});await page.evaluate(()=>term.resize(50,24));
         }
         await sleep(900);leave();await sleep(300);const compact=await shot('compact-real-branch');
         const railWidth=binary.origin==='upstream'?5:6;
         const railText=compact.cells.map(row=>row.slice(0,railWidth).map(cell=>cell.symbol).join('')).join('\n');
         assert(!railText.includes('VIS41')&&!compact.text.includes('VIS41 long-title'),'Actual branch still painted the long tab title');
         move(3,3,'compact-tab-hover');await sleep(1000);await shot('compact-hover');await idle('compact-motionless-idle');
         const after=await ask('owner_snapshot','compact-after');
         assert.deepEqual(rootRows(before),rootRows(after),'Compact UI modified root owner rows');
         assert.equal(before.requests,after.requests);assert.equal(before.completed,after.completed);assert.deepEqual(before.project_files,after.project_files);
         report.compact={route:binary.origin==='upstream'?'real public rail drag 42→5':'real terminal resize 120/160→50 with released 44-cell content minimum',
           columns:compact.columns,rows:compact.rows,observed_title_absent_in_rail:true,rail_width:railWidth,
           requests_unchanged:true,root_rows_unchanged:true,project_unchanged:true,
           paired_qualification:'Different actual public routes/viewports; diagnostic only. No same-viewport compact parity or invented native rail-width control.'};
         if(binary.origin==='upstream'){
           send('\x1b[<0;5;1M','compact-restore-rail-down');send('\x1b[<32;42;1M','compact-restore-rail-drag');send('\x1b[<0;42;1m','compact-restore-rail-up');
         }else{
           control({kind:'resize',columns,rows});await page.evaluate(({columns,rows})=>term.resize(columns,rows),{columns,rows});
         }
         await sleep(500);leave();await sleep(150);await shot('compact-restored');checkpoint();
       }
      // Actual PTY resize: the same long tab is hovered, then all approved profiles.
      for(const [index,[c,r]]of [[80,24],[120,40],[160,48],[columns,rows]].entries()){
        stage=`resize-${index}-${c}x${r}`;control({kind:'resize',columns:c,rows:r});await page.evaluate(({c,r})=>term.resize(c,r),{c,r});await wait(f=>f.columns===c&&f.rows===r,'frontend resize');await sleep(350);await shot(stage);
      }
      await idle('final-settled-idle');send('\x04','clean_exit');await wait(()=>logs.some(e=>e.kind==='exit'),'natural app exit',12000);
      assert(logs.some(e=>e.kind==='exit'&&e.code===0&&e.termination==='natural'),'Non-natural application exit');
      report.status='PASS_BOUNDED_OWNER_AND_OBSERVATIONS';
    }catch(e){
      report.status='FAILED';report.reason=e.message;console.error(name+': '+e.message);
      try{await shot('failure',{failed_stage:stage,reason:e.message});}catch(captureError){report.failure_capture_error=captureError.message;}
    }finally{
      watcher=undefined;watcherMarkers=[];if(exit===undefined)control({kind:'stop'});await closed;await queue;
      // Full styled timeline is losslessly decoded after the real process exits.
      // Source timestamps/byte boundaries are preserved. Replay changes no app
      // clock and cannot create observations or synthetic intermediate frames.
      if(timeline==='all')await page.evaluate(({columns,rows})=>{window.terminalReply=async()=>{};term.reset();term.resize(columns,rows);},{columns,rows});
      for(let begin=0;timeline==='all'&&begin<chunks.length;begin+=8){
        const batch=chunks.slice(begin,begin+8).map((chunk,index)=>({...chunkEvents[begin+index],data:chunk.toString('base64')}));
        const decoded=await page.evaluate(async batch=>{
          const frames=[];
          for(const event of batch){
            if(!event.columns)continue;
            if(term.cols!==event.columns||term.rows!==event.rows)term.resize(event.columns,event.rows);
            await writeTerminal(event.data);frames.push({source_at_ns:event.at_ns,frontend_at_ns:event.frontend_at_ns,raw_byte_end:event.byte_end,stage:event.stage,...readTerminal()});
          }
          return frames;
        },batch);
        for(const f of decoded)gzip.write(JSON.stringify({decode:'post-exit exact raw VT chunk replay, no application clock',frame_sha256:sha(JSON.stringify(f)),...f})+'\n');
      }
      gzip.end();await timelineClosed;
      save(dir,'protocol.json',logs);save(dir,'inputs.json',inputs);save(dir,'output-chunks.json',chunkEvents);save(dir,'observations.json',observations);
      fs.writeFileSync(path.join(dir,'bridge.stderr.txt'),stderr,{flag:'wx'});save(dir,'checks.json',report);
      const nativeMetrics=path.join(dir,'native-ui-metrics.json');if(binary.origin==='oc'&&fs.existsSync(nativeMetrics))report.native_metrics=JSON.parse(fs.readFileSync(nativeMetrics));
      const files=fs.readdirSync(dir).filter(file=>fs.statSync(path.join(dir,file)).isFile());
      save(dir,'seal.json',{files:Object.fromEntries(files.map(file=>[file,sha(fs.readFileSync(path.join(dir,file)))])),exit:logs.find(e=>e.kind==='exit')});
      lock.attempts.push({name,status:report.status,reason:report.reason||null,counts:counts(),seal_sha256:sha(fs.readFileSync(path.join(dir,'seal.json')))});await page.close();
    }
  }
}finally{
  if(browser)await browser.close();lock.native_source_manifest_after=sources();
  lock.native_source_changed=JSON.stringify(lock.native_source_manifest_before)!==JSON.stringify(lock.native_source_manifest_after);
  lock.campaign_complete=lock.attempts.length===lock.planned_attempts;
  save(output,'capture.lock.json',lock);save(output,'seal.json',{files:Object.fromEntries(['capture.lock.json','tooling.package-lock.json'].map(file=>[file,sha(fs.readFileSync(path.join(output,file)))]))});
}
console.log(JSON.stringify({output,captures:lock.captures.length,attempts:lock.attempts,native_capture_gate:lock.native_capture_gate}));
if(lock.attempts.some(attempt=>attempt.status==='FAILED')||(side!=='reference'&&lock.native_source_changed))process.exitCode=1;
