#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// VIS38 source-derived display: public import into pinned OC2, actual U34 PTY.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {spawn} from 'node:child_process';
import readline from 'node:readline';
import {fileURLToPath} from 'node:url';

const here=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(here,'../..');
const args=Object.fromEntries(process.argv.slice(2).map((v,i,a)=>v.startsWith('--')?[v.slice(2),a[i+1]]:null).filter(Boolean));
for(const key of Object.keys(args))assert(['oracle','output','reference','tools','case','columns','rows','native-spec','display-fixture','profiles'].includes(key),'Unknown option '+key);
const tools=path.resolve(args.tools||'/home/opencode/.cache/opencode-tmp/opencode/t44-reference');
const output=path.resolve(args.output||''),oracle=path.resolve(args.oracle||'');
const outputParent=path.dirname(output),approved='/home/opencode/.cache/opencode-tmp/opencode';
assert(outputParent===path.join(repo,'evidence/tui/recovery-v00')||
  path.dirname(outputParent)===approved&&/^vis38-durable-current-[0-9]{3}$/.test(path.basename(outputParent)));
const nativeSpec=args['native-spec']?JSON.parse(fs.readFileSync(args['native-spec'])):null;
assert.match(path.basename(output),nativeSpec?/^dcp-nativecapture[a-z0-9-]+$/:/^dcp-reference[a-z0-9-]+$/);
const binary=nativeSpec?.binary||args.reference;
assert(binary&&path.isAbsolute(binary),'Explicit pinned absolute binary required');
const sha=data=>createHash('sha256').update(data).digest('hex');
assert.equal(sha(fs.readFileSync(binary)),nativeSpec?.binary_sha256||'2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a');
const goldens=nativeSpec?path.resolve(args['native-spec']):path.join(oracle,'goldens.json');
const oracleResult=nativeSpec?{status:'PASS_PINNED_DISPLAY_ORACLE',goldens_sha256:sha(fs.readFileSync(goldens))}:JSON.parse(fs.readFileSync(path.join(oracle,'result.json')));
const goldenData=nativeSpec?{provenance:{native_owner:true},notifications:[{id:'native-'+nativeSpec.stage,native_context:{capture_after:nativeSpec.capture_after},payload:null}]}:JSON.parse(fs.readFileSync(goldens));
const displayFixture=args['display-fixture']?JSON.parse(fs.readFileSync(args['display-fixture'])):null;
if(displayFixture){
  assert(!nativeSpec,'Reference-only transfer must never enter native capture');
  assert.equal(displayFixture.goldens_sha256,sha(fs.readFileSync(goldens)));
  assert.equal(displayFixture.case,args.case);
}
if(args.profiles)assert.equal(args.profiles,'all');
assert.equal(oracleResult.status,'PASS_PINNED_DISPLAY_ORACLE');
assert.equal(oracleResult.goldens_sha256,sha(fs.readFileSync(goldens)));
fs.mkdirSync(output); // immutable attempt
const save=(file,value)=>fs.writeFileSync(path.join(output,file),JSON.stringify(value,null,2)+'\n',{flag:'wx'});
const root=path.join(tools,'runs',path.basename(output)+'-'+sha(output).slice(0,12));
fs.mkdirSync(root);
const env={PATH:'/usr/bin:/bin',HOME:path.join(root,'browser-home'),LANG:'C.UTF-8',LC_ALL:'C.UTF-8',TZ:'UTC',PLAYWRIGHT_BROWSERS_PATH:path.join(tools,'browsers')};
fs.mkdirSync(env.HOME);
process.env.PLAYWRIGHT_BROWSERS_PATH=env.PLAYWRIGHT_BROWSERS_PATH;
const require=createRequire(path.join(tools,'package.json')),{chromium}=require('playwright');
fs.copyFileSync(path.join(tools,'package-lock.json'),path.join(output,'tooling.package-lock.json'));
const requests=nativeSpec?.durable_lifecycle?[{case:'native-durable',columns:120,rows:40}]:args.case?(args.profiles==='all'?[[80,24],[120,40],[160,48]].map(([columns,rows])=>({case:args.case,columns,rows})):[{case:args.case,columns:Number(args.columns||120),rows:Number(args.rows||40)}]):goldenData.provenance.native_owner?
  goldenData.notifications.flatMap(g=>[[80,24],[120,40],[160,48]].map(([columns,rows])=>({case:g.id,columns,rows}))):[
  ...['detailed','minimal','off','show-single','multi-range','zero-summary-tools','recompression','large-k-only'].map(c=>({case:c,columns:120,rows:40})),
  {case:'detailed',columns:80,rows:24},{case:'detailed',columns:160,rows:48},
  {case:'long-topic-summary',columns:80,rows:24},{case:'long-topic-summary',columns:160,rows:48},
];
for(const r of requests)assert([[80,24],[120,40],[160,48]].some(([c,h])=>c===r.columns&&h===r.rows),'Use an approved profile');
const lock={schema_version:1,method:nativeSpec?'Released native binary -> genuine held owner session -> real PTY, no import':'source-derived display; intercepted pinned DCP payload -> original public session import -> unmodified U34 renderer',qualification:'Capture integrity only; no full parity claim',binary:{path:binary,sha256:sha(fs.readFileSync(binary)),commit:nativeSpec?.source_HEAD||'2670273ff17da96f85c5826ced57aa1b368754fa'},goldens_sha256:sha(fs.readFileSync(goldens)),source_manifest:nativeSpec||JSON.parse(fs.readFileSync(path.join(oracle,'source-manifest.json'))),runner_hashes:Object.fromEntries(['dcp_reference_capture.mjs',nativeSpec?'dcp_native_bridge.py':'dcp_reference_bridge.py','frontend.js'].map(n=>[n,sha(fs.readFileSync(path.join(here,n)))])),captures:[],attempts:[]};
if(displayFixture)lock.display_fixture={path:path.resolve(args['display-fixture']),sha256:sha(fs.readFileSync(args['display-fixture'])),owner:displayFixture.owner};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
let browser;
try {
  browser=await chromium.launch({headless:true,env});
  lock.frontend={chromium:browser.version(),playwright:require('playwright/package.json').version,xterm:require('@xterm/xterm/package.json').version,unicode:require('@xterm/addon-unicode11/package.json').version,font:'DejaVu Sans Mono',font_size:14,device_scale_factor:1,padding:0,unmasked:true};
  for(const request of requests) {
    const entry=goldenData.notifications.find(g=>g.id===request.case);
    assert(entry,'Unknown notification case '+request.case);
    const name=`${request.case}-${request.columns}x${request.rows}`,dir=path.join(output,name);
    fs.mkdirSync(dir);
    const localSave=(name,value)=>fs.writeFileSync(path.join(dir,name),JSON.stringify(value,null,2)+'\n',{flag:'wx'});
    const spec={binary,...request,goldens,isolated_root:path.join(root,name),...(nativeSpec?{native_spec:path.resolve(args['native-spec'])}:{}),...(displayFixture?{display_fixture:path.resolve(args['display-fixture'])}:{})};
    localSave('bridge-spec.json',spec);
    const page=await browser.newPage({viewport:{width:1800,height:1100},deviceScaleFactor:1});
    await page.setContent('<style>html,body{margin:0;background:#0a0a0a}#terminal{display:inline-block;font-variant-ligatures:none}.xterm-viewport{scrollbar-width:none}</style><div id="terminal"></div>');
    await page.addStyleTag({path:path.join(tools,'node_modules/@xterm/xterm/css/xterm.css')});
    await page.addScriptTag({path:path.join(tools,'node_modules/@xterm/xterm/lib/xterm.js')});
    await page.addScriptTag({path:path.join(tools,'node_modules/@xterm/addon-unicode11/lib/addon-unicode11.js')});
    await page.addScriptTag({path:path.join(here,'frontend.js')});
    const child=spawn('/usr/bin/python3',[path.join(here,nativeSpec?'dcp_native_bridge.py':'dcp_reference_bridge.py'),path.join(dir,'bridge-spec.json')],{env,stdio:['pipe','pipe','pipe']});
    const logs=[],inputs=[],chunks=[],observers=new Map();
    let queue=Promise.resolve(),exit,stderr='',writeError;
    const closed=new Promise(resolve=>child.once('close',resolve));
    child.on('exit',code=>{exit=code;});
    child.stderr.on('data',b=>{stderr+=b.toString();});
    const send=(data,kind='terminal_reply')=>{
      inputs.push({at_ms:Date.now(),kind,base64:Buffer.from(data).toString('base64')});
      if(!child.stdin.destroyed)child.stdin.write(JSON.stringify({kind:'input',data:Buffer.from(data).toString('base64')})+'\n');
    };
    await page.exposeFunction('terminalReply',data=>send(data));
    await page.evaluate(p=>startTerminal(p),request);
    readline.createInterface({input:child.stdout}).on('line',line=>{
      const event=JSON.parse(line);
      if(event.kind==='output'){
        const bytes=Buffer.from(event.data,'base64');chunks.push(bytes);
        fs.appendFileSync(path.join(dir,'raw.vt'),bytes);
        queue=queue.then(()=>page.evaluate(d=>writeTerminal(d),event.data)).catch(e=>{writeError=e;});
      }else {logs.push(event);if(event.kind==='owner_snapshot')observers.get(event.label)?.(event);}
    });
    const frame=async()=>{await queue;if(writeError)throw writeError;return page.evaluate(()=>readTerminal());};
    const settle=async(predicate,label)=>{
      const end=Date.now()+60000;
      let previous,stable=0;
      while(Date.now()<end){
        const f=await frame(),hash=sha(JSON.stringify(f));stable=hash===previous?stable+1:0;previous=hash;
        if(predicate(f)&&stable>=4)return f;
        if(exit!==undefined)throw Error('Bridge exit '+exit+' waiting '+label+': '+stderr);
        await sleep(200);
      }
      throw Error('Timeout '+label);
    };
    const shot=async(stage,f)=>{
      await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
      const rect=await page.locator('.xterm-screen').boundingBox();
      assert(rect);
      const before=await page.evaluate(()=>readCaptureGeometry());
      const png=await page.screenshot({path:path.join(dir,stage+'.png'),clip:{x:rect.x,y:rect.y,width:Math.ceil(rect.width),height:Math.ceil(rect.height)}});
      const after=await frame();
      assert.equal(sha(JSON.stringify(after)),sha(JSON.stringify(f)),'Grid changed during full PNG');
      const {text,...grid}=f;
       localSave(stage+'.cells.json',{schema_version:1,origin:nativeSpec?'oc':'upstream',scenario:`vis38-${name}-${stage}`,fixture_sha256:lock.goldens_sha256,producer_commit:lock.binary.commit,...grid});
      fs.writeFileSync(path.join(dir,stage+'.txt'),text+'\n',{flag:'wx'});
      fs.writeFileSync(path.join(dir,stage+'.vt'),Buffer.concat(chunks),{flag:'wx'});
      localSave(stage+'.render.json',{before,after:await page.evaluate(()=>readCaptureGeometry()),png_width:png.readUInt32BE(16),png_height:png.readUInt32BE(20),cursor:f.cursor,unmasked:true});
      lock.captures.push({name,stage,columns:request.columns,rows:request.rows,files:Object.fromEntries(['cells.json','png','txt','vt','render.json'].map(ext=>[ext,sha(fs.readFileSync(path.join(dir,stage+'.'+ext)))]))});
    };
    const owner=label=>new Promise((resolve,reject)=>{
      const timeout=setTimeout(()=>reject(Error('Owner observation timeout '+label)),60000);
      assert(!observers.has(label));observers.set(label,event=>{clearTimeout(timeout);observers.delete(label);resolve(event);});
      child.stdin.write(JSON.stringify({kind:'observe',label})+'\n');
    });
    try {
       const after=entry.native_context?.capture_after||'VIS38-AFTER';
       const f=await settle(f=>f.text.includes(after),'imported session at bottom');
       if(!nativeSpec)assert(logs.some(e=>e.kind==='exact_import_export_verified'),'Missing exact export proof');
      assert(!logs.some(e=>e.kind==='unexpected_provider_request'),'Imported display called provider');
      if(request.case==='off')assert(!f.text.includes('▣ DCP'));
       else if(request.case!=='long-topic-summary'&&!entry.native_context)assert(f.text.includes('▣ DCP'),'Notification not visible');
       await shot('bottom',f);
       const shown=entry.native_context?.display||nativeSpec?.display;
        if(!nativeSpec?.durable_lifecycle&&entry.native_context&&(!shown||(shown.notification==='detailed'&&shown.channel==='chat'))){
         const ordinal=nativeSpec?.typed_snapshot?.ordinal||entry.native_context.typed_snapshot?.ordinal;
         const needle='▣ Compression #'+ordinal+' ';
         let card=f;
         // Navigation only: preserve the full viewport; never crop or mask a card.
         for(let i=0;i<16&&(!card.text.includes(needle)||!card.text.includes('▣ DCP'));i++){
           send('\x1b[<64;10;5M','card_wheel_up');
           card=await settle(f=>f.text!==card.text,'card scroll');
            await shot('scroll-'+i,card);
         }
        }
        if(nativeSpec?.durable_lifecycle){
         const lifecycle=nativeSpec.durable_lifecycle,session=nativeSpec.session;
         const selected=(s,table)=>s.data[table].filter(r=>!('session_id' in r)||r.session_id===session);
         const runs=s=>selected(s,'dcp_run_views').map(r=>JSON.parse(r.snapshot));
         const state=async label=>{const f=await settle(()=>true,label);await shot(label,f);return owner(label);};
         const raw=await owner('reopened'),rawMessages=selected(raw,'messages'),rawOps=selected(raw,'tool_operations');
         const immutable=s=>{assert.deepEqual(selected(s,'messages'),rawMessages);assert.deepEqual(selected(s,'tool_operations'),rawOps);};
         const projection=s=>Object.fromEntries(['conversation_messages','compression_blocks','compression_members','dcp_run_views','dcp_accounting','dcp_coverage','conversation_tools'].map(t=>[t,selected(s,t)]));
         const original=projection(raw);assert.equal(runs(raw).length,3);
         // Exact owner-derived display labels, traced to oc-tui/dcp_view.rs.
         const tokens=n=>{assert(Number.isSafeInteger(n)&&n>=0);if(n<1000)return String(n);
           const unit=n<999950?1000:1000000,suffix=unit===1000?'K':'M';
           const tenths=Math.floor(n/unit)*10+Math.floor(((n%unit)+unit/20)/(unit/10));
           return (tenths%10?`${Math.floor(tenths/10)}.${tenths%10}`:String(tenths/10))+suffix;};
         const metrics=(removed,summary)=>`-${tokens(removed)} removed`+(summary?`, +${tokens(summary)} summary`:'');
         const card=async s=>{const r=runs(s).sort((a,b)=>a.ordinal-b.ordinal).at(-1),f=await frame();
           assert(f.text.includes('▣ DCP | '+metrics(r.cumulative.gross_removed,r.cumulative.active_summary)));
           assert(f.text.includes('▣ Compression #'+r.ordinal+' '+metrics(r.removed,r.summary)));
           assert(f.text.includes('│'+r.bar+'│'),'Stored frozen bar must be displayed exactly');};
         await card(raw);
         const panel=async(label,s)=>{
           const accounting=selected(s,'dcp_accounting').map(r=>JSON.parse(r.snapshot));assert.equal(accounting.length,1);
           const active=selected(s,'compression_blocks').filter(b=>b.hot?JSON.parse(b.hot).active:
             selected(s,'compression_members').some(m=>m.block_id===b.id)).length;
           send('/dcp\r','public_dcp_'+label);
           const text=`blocks ${active} | compressions ${accounting[0].compressions} | nudges 0 | prunes ${accounting[0].prunes}`;
           const f=await settle(f=>f.text.includes(text),'owner-derived DCP panel '+label);await shot(label+'-dcp',f);
           assert(f.text.includes('UTF-16 length / 4, half-up fallback'));
           const a=accounting[0];assert(f.text.includes(`removed ${tokens(a.gross_removed)} | active summary ${tokens(a.active_summary)} | net saved ${tokens(a.net_saved)} tokens`));
           send('\x1b','close_dcp');await settle(f=>!f.text.includes(text),'close DCP panel');
           const after=await owner(label+'-dcp');assert.deepEqual(selected(after,'messages'),selected(s,'messages'));
           assert.deepEqual(selected(after,'tool_operations'),selected(s,'tool_operations'));assert.deepEqual(projection(after),projection(s));
         };
         await panel('reopened',raw);
         send('\x18l','public_other_session_picker');await shot('other-session-picker',await settle(f=>f.text.includes('Sessions for project'),'other session picker'));
         send(lifecycle.other_title,'public_other_search');await settle(f=>f.text.includes(lifecycle.other_title),'other title search');send('\r','public_select_other');
         const other=await state('other-session');immutable(other);
         assert(other.data.sessions.some(s=>s.id===lifecycle.other_session));
         assert(!other.data.dcp_run_views.some(r=>r.session_id===lifecycle.other_session));
         send('/dcp\r','public_other_dcp');const empty=await settle(f=>f.text.includes('blocks 0 | compressions 0 | nudges 0 | prunes 0'),'other session empty DCP');await shot('other-session-dcp',empty);
         send('\x1b','close_other_dcp');await settle(f=>!f.text.includes('blocks 0 | compressions 0'),'close other DCP');
         send('\x18l','public_sessions');await shot('session-picker',await settle(f=>f.text.includes('Sessions for project'),'session picker'));
         send(lifecycle.session_title,'public_session_search');await settle(f=>f.text.includes(lifecycle.session_title),'owner title search');
         send('\r','public_resume_session');const resumed=await state('resumed');immutable(resumed);assert.deepEqual(projection(resumed),original);
         await card(resumed);
         await panel('resumed',resumed);
         send('\x18u','public_undo_restart');const undoRestart=await state('undo-restart');immutable(undoRestart);assert.equal(runs(undoRestart).length,3);
         send('\x03','clear_restored_draft');await settle(()=>true,'draft cleared');
         send('\x18u','public_undo_compression');const undone=await state('undo-compression');immutable(undone);
         assert.deepEqual(runs(undone).map(r=>r.ordinal).sort(),[1,2]);assert.equal(selected(undone,'conversation_tools').length,2);
         await card(undone);
         send('\x03','clear_restored_draft');await settle(()=>true,'draft cleared');await panel('undone',undone);
         assert(!((await frame()).text.includes('▣ Compression #3 ')),'Undone card is still displayed');
         send('\x18r','public_redo');const redone=await state('redone');immutable(redone);assert.deepEqual(projection(redone),original);await card(redone);await panel('redone',redone);
         send('\x18u','public_undo_restart_again');immutable(await state('undo-restart-again'));
         send('\x03','clear_restored_draft');await settle(()=>true,'draft cleared');
         send('\x18u','public_undo_compression_again');const branchBase=await state('branch-base');immutable(branchBase);assert.deepEqual(projection(branchBase),projection(undone));
         send('\x03','clear_restored_draft');await settle(()=>true,'draft cleared');
         send(lifecycle.branch_prompt+'\r','public_new_branch');await settle(f=>f.text.includes(lifecycle.branch_prompt)&&f.text.includes('VIS38-NATIVE-CONTINUED'),'new branch continuation');
         const branched=await state('branched');assert(rawMessages.every(r=>selected(branched,'messages').some(a=>JSON.stringify(a)===JSON.stringify(r))));
         assert.equal(selected(branched,'messages').length,rawMessages.length+2);assert.deepEqual(selected(branched,'tool_operations'),rawOps);
         assert.deepEqual(runs(branched).map(r=>r.ordinal).sort(),[1,2]);assert(!selected(branched,'conversation_redo').length);
         assert(selected(branched,'conversation_points').some(p=>p.active===0));assert(selected(branched,'conversation_exclusions').length);
         assert.deepEqual(selected(branched,'dcp_accounting'),selected(undone,'dcp_accounting'));await card(branched);await panel('branched',branched);
         lock.durable_lifecycle={status:'PASS_PUBLIC_DURABLE_BRANCH',session,observations:logs.filter(e=>e.kind==='owner_snapshot').map(e=>e.label),
           original_ordinals:[1,2,3],branch_ordinals:[1,2],raw_preserved:true,immutable_operations_preserved:true,
           readonly_observer:true,undo_redo_projection_exact:true,session_switch_projection_exact:true};
        }
      if(request.case==='long-topic-summary'){
        send('\x07','session_first_ctrl_g');
        const top=await settle(f=>f.text.includes('VIS38-BEFORE')&&f.text.includes('▣ DCP'),'long summary top');
        await shot('top',top);
      }
      send('\x03','clean_exit');
      const end=Date.now()+10000;
      while(!logs.some(e=>e.kind==='exit')&&Date.now()<end)await sleep(100);
      assert(logs.some(e=>e.kind==='exit'&&e.code===0&&e.termination==='natural'),'No clean original exit');
      assert(!logs.some(e=>e.kind==='unexpected_provider_request'),'Display emitted a provider request');
       lock.attempts.push({name,status:nativeSpec?'PASS_NATIVE_STAGE_CAPTURE':'PASS_REFERENCE_CAPTURE',...(nativeSpec?{genuine_session:true}:{provider_requests:0,import_export_exact:true})});
    }catch(error){
      lock.attempts.push({name,status:'FAILED_REFERENCE_CAPTURE',reason:error.message});
       console.error(name+': '+error.stack);
      if(chunks.length){const f=await frame();await shot('failure',f);}
    }finally{
      if(exit===undefined&&!child.stdin.destroyed)child.stdin.write(JSON.stringify({kind:'stop'})+'\n');
      await closed;
      localSave('protocol.json',logs);localSave('inputs.json',inputs);
      fs.writeFileSync(path.join(dir,'bridge.stderr.txt'),stderr,{flag:'wx'});
      await page.close();
    }
  }
}finally{
  if(browser)await browser.close();
  save('capture.lock.json',lock);
}
console.log(JSON.stringify({output,captures:lock.captures.length,attempts:lock.attempts}));
if(lock.attempts.some(a=>a.status!==(nativeSpec?'PASS_NATIVE_STAGE_CAPTURE':'PASS_REFERENCE_CAPTURE')))process.exitCode=1;
