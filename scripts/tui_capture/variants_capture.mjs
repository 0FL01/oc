#!/usr/bin/env node
// VIS09/VIS29 presentation: same explicit fixture, real public actions and full VT.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {spawn,spawnSync} from 'node:child_process';
import readline from 'node:readline';
import {fileURLToPath} from 'node:url';

const here=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(here,'../..');
const args={};for(let i=2;i<process.argv.length;i+=2){assert(process.argv[i].startsWith('--')&&process.argv[i+1]);args[process.argv[i].slice(2)]=process.argv[i+1];}
for(const key of Object.keys(args))assert(['output','oc','native-sha256','reference','columns','rows','order'].includes(key));
const output=path.resolve(args.output||'');
assert.equal(path.dirname(output),path.join(repo,'evidence/tui/recovery-v00'));
assert.match(path.basename(output),/^variants-paired[a-z0-9-]+$/);
const sha=b=>createHash('sha256').update(b).digest('hex');
const save=(dir,name,value)=>fs.writeFileSync(path.join(dir,name),JSON.stringify(value,null,2)+'\n',{flag:'wx'});
const tools='/home/opencode/.cache/opencode-tmp/opencode/t44-reference';
const order=args.order||'canonical';assert(['canonical','unsorted'].includes(order));
const profiles=args.columns?[[Number(args.columns),Number(args.rows)]]:[[80,24],[120,40],[160,48]];
assert(profiles.every(([c,r])=>[[80,24],[120,40],[160,48]].some(([x,y])=>c===x&&r===y)));
const canonical=['none','minimal','fast','low','medium','high','xhigh','max','zeta','alpha'];
const declared=order==='canonical'?canonical:['max','xhigh','zeta','fast','high','alpha','medium','low','minimal','none'];
const values={none:{reasoningEffort:'none'},minimal:{reasoningEffort:'minimal'},fast:{reasoningEffort:'low'},low:{},
  medium:{reasoningEffort:'medium'},high:{reasoningEffort:'high'},xhigh:{reasoningEffort:'xhigh'},max:{reasoningEffort:'max'},
  zeta:{reasoningEffort:' deep '},alpha:{reasoningEffort:'custom'}};
// Independently supplied literal contract order, never extracted from Rust output.
const fixture={title:'VIS09 Presentation',initial_model:'vis09-model-03',chosen_model:'vis09-model-15',provider_name:'VIS09 Fixture',
  declared,canonical,models:Object.fromEntries(Array.from({length:16},(_,i)=>['vis09-model-'+String(i).padStart(2,'0'),{
    name:'VIS09 Model '+String(i).padStart(2,'0'),limit:{context:65536,output:2048},
    cost:{input:[0,3].includes(i)?0:1,output:[0,3].includes(i)?0:2},variants:Object.fromEntries(declared.map(n=>[n,values[n]]))}])),
  cli:{theme:{name:'opencode',mode:'dark'},animations:false,session:{sidebar:'hide',tps:false},tabs:{layout:'horizontal'},
    keybinds:{'model.dialog.provider':'none'},
    debug:{devtools:false},attention:{notifications:false,sound:false},cursor:{style:'block',blinking:false}}};
const head=spawnSync('git',['rev-parse','HEAD'],{cwd:repo,encoding:'utf8'}).stdout.trim();
const files=spawnSync('git',['ls-files','crates','Cargo.toml','Cargo.lock','rust-toolchain.toml'],{cwd:repo,encoding:'utf8'}).stdout.trim().split('\n');
const source=()=>Object.fromEntries(files.map(p=>[p,sha(fs.readFileSync(path.join(repo,p)))]));
const sources=source();
assert.equal(sha(fs.readFileSync(args.oc)),args['native-sha256']);
assert.equal(sha(fs.readFileSync(args.reference)),'2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a');
fs.mkdirSync(output);save(output,'fixture.json',fixture);
const root=path.join(tools,'runs',path.basename(output));fs.mkdirSync(root);
const env={HOME:path.join(root,'browser-home'),PATH:'/usr/bin:/bin',LANG:'C.UTF-8',LC_ALL:'C.UTF-8',TZ:'UTC',PLAYWRIGHT_BROWSERS_PATH:path.join(tools,'browsers')};fs.mkdirSync(env.HOME);
process.env.PLAYWRIGHT_BROWSERS_PATH=env.PLAYWRIGHT_BROWSERS_PATH;
const require=createRequire(path.join(tools,'package.json')),{chromium}=require('playwright');
fs.copyFileSync(path.join(tools,'package-lock.json'),path.join(output,'tooling.package-lock.json'));
const lock={schema_version:1,scope:'T44 VIS09/VIS29',order,source_HEAD:head,native_sources_before:sources,
  native_sha256:args['native-sha256'],reference_sha256:sha(fs.readFileSync(args.reference)),reference_commit:'2670273ff17da96f85c5826ced57aa1b368754fa',
  fixture_sha256:sha(fs.readFileSync(path.join(output,'fixture.json'))),commands:process.argv,profiles,attempts:[],captures:[],
  helpers:Object.fromEntries(['variants_capture.mjs','variants_bridge.py','frontend.js'].map(n=>[n,sha(fs.readFileSync(path.join(here,n)))])),
  reference_sources:Object.fromEntries(['component/dialog-model.tsx','component/dialog-variant.tsx','model-preference.ts','context/local.tsx','ui/dialog-select.tsx'].map(n=>[n,sha(fs.readFileSync(path.join(repo,'opencode/packages/tui/src',n)))])),
  mapping:'Native real public rename of genuinely empty session -> original public transfer/import/export only. No native history/pref injection; no masks, row normalization, version replacement or donor rewriting.',
  shared_behavior:'evidence/T47/canonical-effort.md; implementation 6cf1cdc859',qualification:'Per-frame full styled-cell/cursor/PNG equality required for canonical. Unsorted is an approved order difference, never global pixel PASS.'};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));let browser;
try{
  browser=await chromium.launch({headless:true,env});
  lock.frontend={chromium:browser.version(),playwright:require('playwright/package.json').version,xterm:require('@xterm/xterm/package.json').version,
    unicode:require('@xterm/addon-unicode11/package.json').version,font:'DejaVu Sans Mono',font_size:14,device_scale_factor:1,padding:0,unmasked:true};
  for(const [columns,rows] of profiles){
    const project=path.join(root,`${columns}x${rows}-shared`,'project'),nativeTransfer=path.join(output,`${columns}x${rows}-native-transfer.json`);
    for(const origin of ['oc','upstream']){
      const name=`${origin}-${columns}x${rows}`,dir=path.join(output,name);fs.mkdirSync(dir);
      const spec={origin,columns,rows,binary:origin==='oc'?args.oc:args.reference,binary_sha256:origin==='oc'?args['native-sha256']:lock.reference_sha256,
        root:path.join(root,name),project,fixture:path.join(output,'fixture.json'),native_transfer:nativeTransfer};save(dir,'bridge-spec.json',spec);
      const page=await browser.newPage({viewport:{width:1800,height:1100},deviceScaleFactor:1});
      await page.setContent('<style>html,body{margin:0;background:#0a0a0a}#terminal{display:inline-block;font-variant-ligatures:none}.xterm-viewport{scrollbar-width:none}</style><div id="terminal"></div>');
      await page.addStyleTag({path:path.join(tools,'node_modules/@xterm/xterm/css/xterm.css')});
      for(const n of ['node_modules/@xterm/xterm/lib/xterm.js','node_modules/@xterm/addon-unicode11/lib/addon-unicode11.js'])await page.addScriptTag({path:path.join(tools,n)});
      await page.addScriptTag({path:path.join(here,'frontend.js')});
      const child=spawn('/usr/bin/python3',[path.join(here,'variants_bridge.py'),path.join(dir,'bridge-spec.json')],{env,stdio:['pipe','pipe','pipe']});
      const closed=new Promise(r=>child.once('close',r)),logs=[],inputs=[],chunks=[];
      let queue=Promise.resolve(),stderr='',exit,error;
      child.on('exit',code=>{exit=code;});child.stderr.on('data',b=>{stderr+=b;});child.stdin.on('error',e=>{error=e;});
      const control=value=>{if(!child.stdin.destroyed)child.stdin.write(JSON.stringify(value)+'\n');};
      const send=(data,label='terminal_reply')=>{inputs.push({at_ns:process.hrtime.bigint().toString(),label,base64:Buffer.from(data).toString('base64')});control({kind:'input',data:Buffer.from(data).toString('base64')});};
      await page.exposeFunction('terminalReply',data=>send(data));await page.evaluate(p=>startTerminal(p),{columns,rows});
      await page.evaluate(()=>{const original=window.readTerminal;window.readTerminal=()=>{const frame=original();frame.cursor.color='#'+(term._core._themeService.colors.cursor.rgba>>>8).toString(16).padStart(6,'0');return frame;};});
      readline.createInterface({input:child.stdout}).on('line',line=>{
        const event=JSON.parse(line);if(event.kind==='output'){
          const bytes=Buffer.from(event.data,'base64');chunks.push(bytes);fs.appendFileSync(path.join(dir,'raw.vt'),bytes);
          if(chunks.reduce((n,b)=>n+b.length,0)>16*1024*1024)error=Error('Raw log cap');
          queue=queue.then(()=>page.evaluate(d=>writeTerminal(d),event.data)).catch(e=>{error=e;});
        }else logs.push(event);
      });
      const frame=async()=>{await queue;if(error)throw error;return page.evaluate(()=>readTerminal());};
      const wait=async(predicate,label)=>{
        const end=performance.now()+15000;let previous,stable=0;
        while(performance.now()<end){const f=await frame(),hash=sha(JSON.stringify(f));stable=hash===previous?stable+1:0;previous=hash;
          if(predicate(f)&&stable>=2)return f;if(exit!==undefined)throw Error('Bridge exit '+exit+' waiting '+label+' '+stderr);await sleep(80);}
        throw Error('Timeout '+label);
      };
      const snap=async(label)=>{control({kind:'owner_snapshot',request_id:label});const end=performance.now()+8000;
        while(!logs.some(e=>e.kind==='owner_snapshot'&&e.request_id===label)){assert(performance.now()<end,'Owner snapshot timeout');await sleep(30);}
        return logs.find(e=>e.kind==='owner_snapshot'&&e.request_id===label);};
      const shot=async(label)=>{
        const f=await frame();await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
        const before=await page.evaluate(()=>readCaptureGeometry()),rect=before.screen_rect;
        const png=await page.screenshot({path:path.join(dir,label+'.png'),clip:{x:rect.x,y:rect.y,width:Math.ceil(rect.width),height:Math.ceil(rect.height)}});
        assert.equal(sha(JSON.stringify(await frame())),sha(JSON.stringify(f)),'Grid changed during PNG');
        const {text,...grid}=f;save(dir,label+'.cells.json',{schema_version:1,origin,scenario:`vis09-${order}-${columns}x${rows}-${label}`,
          producer_commit:origin==='oc'?head:lock.reference_commit,fixture_sha256:lock.fixture_sha256,
          environment_id:sha(JSON.stringify({columns,rows,...lock.frontend,cli:fixture.cli})),...grid});
        fs.writeFileSync(path.join(dir,label+'.txt'),text+'\n',{flag:'wx'});fs.writeFileSync(path.join(dir,label+'.vt'),Buffer.concat(chunks),{flag:'wx'});
        save(dir,label+'.render.json',{before,after:await page.evaluate(()=>readCaptureGeometry()),cursor:f.cursor,png_width:png.readUInt32BE(16),png_height:png.readUInt32BE(20),unmasked:true});
        lock.captures.push({name,label,files:Object.fromEntries(['cells.json','png','vt','txt','render.json'].map(ext=>[ext,sha(fs.readFileSync(path.join(dir,label+'.'+ext)))]))});
        return f;
      };
      const dismiss=async(title)=>{send('\x1b','dismiss');await wait(f=>!f.text.includes(title),'dismiss '+title);};
      const openModels=async()=>{send('\x18','model_leader');await sleep(80);send('m','model_open');return wait(f=>f.text.includes('Select model'),'model dialog');};
      const openVariants=async()=>{send('\x10','variant_palette');await wait(f=>f.text.includes('Commands'),'palette');send('Switch model variant','variant_command_query');await sleep(120);send('\r','variant_command_execute');return wait(f=>f.text.includes('Select variant'),'variant dialog');};
      const checks=[];const rowsOf=(s,t)=>s.observations.filter(o=>o.table===t).flatMap(o=>o.rows);
      const selected=(s)=>{
        if(origin==='oc')return rowsOf(s,'prefs').filter(r=>r.key.startsWith('tui.selection.session:')).map(r=>JSON.parse(r.value).models.build);
        return s.client_persistence.filter(r=>r.path.endsWith('/model.json')).flatMap(r=>Object.entries(r.data.variant).map(([key,variant])=>({id:key.split('/')[1],variant:variant==='default'?null:variant,recent:r.data.recent})));
      };
      const verify=async(label,id,variant)=>{const s=await snap(label);assert.equal(s.requests,0,'Selection generated request');
        assert(selected(s).some(v=>v.id===id&&(v.variant??null)===variant),'Exact persisted choice '+label+': '+JSON.stringify(selected(s)));
        if(origin==='upstream'&&['model-selected','final-selection'].includes(label))assert(selected(s).some(v=>v.recent?.[0]?.modelID===id&&v.recent[0].providerID==='fixture'),'Exact original recent model');
        assert.equal(s.project_files.length,0,'Selection changed workspace');
        if(origin==='oc')for(const table of ['conversation_messages','turns','tool_ops','turn_acceptances'])assert.equal(rowsOf(s,table).length,0,'Selection produced '+table);
        checks.push({label,expected:{id,variant},observed:selected(s),snapshot:s});};
      try{
        await wait(f=>f.text.includes('Build · VIS09 Model 03'),'initial prompt');
        if(origin==='oc'){
          send('/rename '+fixture.title,'public_rename');await sleep(100);send('\r','public_rename_submit');
          await wait(f=>f.text.includes(fixture.title)&&!f.text.includes('/rename'),'named empty session');await snap('native-transfer');
        }else assert(logs.some(e=>e.kind==='exact_import_export_verified'));
        send('VIS09 retained draft','draft');await wait(f=>f.text.includes('VIS09 retained draft'),'draft');await shot('normal-default');
        const sequence=origin==='oc'?canonical:declared;
        await openVariants();await shot('variant-initial');await dismiss('Select variant');
        for(const [i,variant] of [...sequence,null].entries()){
          send('\x14','cycle_'+(variant??'Default'));
          await wait(f=>f.text.includes('VIS09 retained draft')&&f.text.includes('Build · VIS09 Model 03 VIS09 Fixture'+(variant?' · '+variant:''))&&!f.text.includes('Select variant'),'cycle '+i);
          await verify('cycle-'+i,fixture.initial_model,variant);await shot('cycle-'+i);
          await openVariants();await shot('cycle-picker-'+i);await dismiss('Select variant');
        }
        await openVariants();send('fast','variant_filter');await wait(f=>f.text.includes('fast')&&!f.text.includes('Default'),'variant query');await shot('variant-filtered');
        send('\r','variant_select_alias');await wait(f=>!f.text.includes('Select variant')&&f.text.includes('Build · VIS09 Model 03 VIS09 Fixture · fast'),'alias selection');
        await verify('picker-alias',fixture.initial_model,'fast');await shot('variant-selected');
        await openVariants();await shot('variant-reopened');send('\x1b[H','variant_home');await sleep(100);send('\r','variant_default');
        await wait(f=>!f.text.includes('Select variant')&&!f.text.includes('VIS09 Fixture · fast'),'variant default');await verify('picker-default',fixture.initial_model,null);
        await openModels();await shot('model-initial');
        for(let i=0;i<13;i++){send('\x1b[B','model_down_'+i);await sleep(100);}await wait(f=>f.text.includes('VIS09 Model 13'),'scrolled model');await shot('model-scrolled');
        send('nonexistent-vis09','model_empty_query');await wait(f=>f.text.includes('No results found'),'model no match');await shot('model-no-match');
        send('\x03','model_clear_query');await sleep(100);send('Model 15','model_query');await wait(f=>f.text.includes('VIS09 Model 15'),'model filtered');await shot('model-filtered');
        send('\r','model_select');await wait(f=>f.text.includes('Select variant'),'model actual variant flow');await shot('model-selected-variant');
        send('\r','new_model_default');await wait(f=>!f.text.includes('Select variant')&&f.text.includes('Build · VIS09 Model 15'),'chosen model');
        await verify('model-selected',fixture.chosen_model,null);await shot('model-selected');
        await openModels();await shot('model-reopened');await dismiss('Select model');await shot('draft-restored');
        await openVariants();send('fast','wire_variant_filter');await sleep(120);send('\r','wire_variant_select');await wait(f=>!f.text.includes('Select variant')&&f.text.includes('Model 15 VIS09 Fixture · fast'),'wire alias');await verify('final-selection',fixture.chosen_model,'fast');
        if(origin==='oc'){
          control({kind:'allow_wire'});send('\r','explicit_wire_submission');
          await wait(f=>f.text.includes('VIS09-WIRE:')&&logs.some(e=>e.kind==='provider_completed'),'actual selected wire');
          assert.equal(logs.filter(e=>e.kind==='provider').length,1);assert(logs.find(e=>e.kind==='provider').valid);
          const completed=await snap('wire-owner');assert(rowsOf(completed,'turns').some(t=>t.status==='completed'),'Actual wire turn completed');
          checks.push({label:'wire',snapshot:completed,request:logs.find(e=>e.kind==='provider')});
        }else{send('\x03','clear_unsent_draft');await wait(f=>!f.text.includes('VIS09 retained draft'),'draft cleared');}
        send('\x03','clean_exit');const end=performance.now()+10000;while(!logs.some(e=>e.kind==='exit')&&performance.now()<end)await sleep(50);
        assert(logs.some(e=>e.kind==='exit'&&e.code===0&&e.termination==='natural'&&e.terminal_restored),'Natural restored exit');
        assert(!logs.some(e=>e.kind==='unexpected_provider_request'));lock.attempts.push({name,status:'PASS_ACTIONS_CAPTURE',checks,requests:logs.filter(e=>e.kind==='provider').length});
      }catch(e){lock.attempts.push({name,status:'FAIL',reason:e.message,checks});console.error(name+': '+e.message);if(chunks.length)await shot('failure').catch(()=>{});}
      finally{
        if(exit===undefined)control({kind:'stop'});await closed;save(dir,'protocol.json',logs);save(dir,'inputs.json',inputs);fs.writeFileSync(path.join(dir,'bridge.stderr.txt'),stderr,{flag:'wx'});await page.close();
      }
    }
  }
}finally{
  if(browser)await browser.close();lock.native_sources_after=source();lock.source_unchanged=JSON.stringify(lock.native_sources_after)===JSON.stringify(sources);
  lock.native_binary_after=sha(fs.readFileSync(args.oc));save(output,'capture.lock.json',lock);
}
console.log(JSON.stringify({output,attempts:lock.attempts.map(({name,status,reason,requests})=>({name,status,reason,requests})),captures:lock.captures.length}));
if(lock.attempts.some(a=>a.status!=='PASS_ACTIONS_CAPTURE')||!lock.source_unchanged)process.exitCode=1;
