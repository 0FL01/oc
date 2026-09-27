#!/usr/bin/env node
// Independent evidence integrity/geometry audit; exit 0 is not native/parity PASS.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';

const [oracleArg,referenceArg,reportArg]=process.argv.slice(2);
assert(oracleArg&&referenceArg&&reportArg,'ORACLE_DIR REFERENCE_DIR NEW_REPORT_JSON');
const oracle=path.resolve(oracleArg),reference=path.resolve(referenceArg),report=path.resolve(reportArg);
assert.equal(path.dirname(report),path.dirname(reference));
assert.match(path.basename(report),/^dcp-reference-check[a-z0-9-]+\.json$/);
const sha=b=>createHash('sha256').update(b).digest('hex'),read=p=>JSON.parse(fs.readFileSync(p));
const manifest=read(path.join(oracle,'source-manifest.json')),goldens=read(path.join(oracle,'goldens.json'));
const hashes={D05:'626a8978c1386ea206ae1600642a0c268e00f6db85b30670049dd2d33ffb319b',D06:'3a7f92b604906530d7494511d07330db3c2b6d8ad695c8cea3bca2b819fec7cb',D07:'ea109d2ae177cb3386d635b0955ce8b65274906a02f20c259e1195ac51769185',D08:'b505a347324507f13f6a2e63d5971e950351be0dce2309ce7e37702e029c6c90',D09:'8206128a441226c0e67f0836646103bbfde616bd7ae15ea5005574fc201def8e',D10:'47e1d1693698f31be16371b7172658d0b446469be0608ed2c1aceabf98e5eaf2',U34:'e4af9db141d101d47a61d4ca761f9c1b38e375288442490ce609732450283734'};
for(const source of manifest.sources){
  assert.equal(sha(fs.readFileSync(path.join(oracle,source.local))),source.sha256,source.id+' source seal');
  if(hashes[source.id])assert.equal(source.sha256,hashes[source.id],source.id+' exact pin hash');
}
const stripImports=text=>text.replace(/^import\s+(?:type\s+)?[\s\S]*?\s+from\s+["'][^"']+["']\s*;?\r?\n/gm,'');
for(const [id,file] of [['D05','notification.ts'],['D06','utils.ts'],['D10','state-utils.ts']]){
  const source=manifest.sources.find(s=>s.id===id);
  assert.equal(stripImports(fs.readFileSync(path.join(oracle,source.local),'utf8')),stripImports(fs.readFileSync(path.join(oracle,'executable',file),'utf8')),id+' formatter/body must be unchanged');
}
const lock=read(path.join(reference,'capture.lock.json'));
assert.equal(lock.binary.sha256,'2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a');
assert.equal(lock.goldens_sha256,sha(fs.readFileSync(path.join(oracle,'goldens.json'))));
assert(lock.attempts.every(a=>a.status==='PASS_REFERENCE_CAPTURE'));
const measurements=[];
for(const capture of lock.captures){
  const dir=path.join(reference,capture.name),stem=path.join(dir,capture.stage);
  for(const [ext,hash] of Object.entries(capture.files))assert.equal(sha(fs.readFileSync(stem+'.'+ext)),hash,capture.name+' '+ext+' seal');
  const grid=read(stem+'.cells.json'),render=read(stem+'.render.json'),png=fs.readFileSync(stem+'.png');
  assert.equal(grid.columns,capture.columns);assert.equal(grid.rows,capture.rows);
  assert.equal(grid.cells.length,grid.rows);
  assert(grid.cells.every(row=>row.length===grid.columns));
  assert(grid.cursor.x>=0&&grid.cursor.x<grid.columns&&grid.cursor.y>=0&&grid.cursor.y<grid.rows);
  assert.equal(fs.readFileSync(stem+'.txt','utf8'),grid.cells.map(r=>r.map(c=>c.symbol).join('')).join('\n')+'\n');
  assert.equal(png.readUInt32BE(16),render.png_width);assert.equal(png.readUInt32BE(20),render.png_height);
  assert.equal(render.png_width,Math.ceil(render.before.screen_rect.width));assert.equal(render.png_height,Math.ceil(render.before.screen_rect.height));
  const spec=read(path.join(dir,'bridge-spec.json')),events=read(path.join(dir,'protocol.json'));
  const fixture=events.find(e=>e.kind==='fixture'),entry=goldens.notifications.find(g=>g.id===spec.case);
  assert(fixture&&entry);
  assert(events.some(e=>e.kind==='exact_import_export_verified'));
  assert(events.some(e=>e.kind==='exit'&&e.code===0&&e.termination==='natural'));
  assert(!events.some(e=>e.kind==='unexpected_provider_request'));
  const importCmd=events.find(e=>e.kind==='command'&&e.argv.includes('import'));
  const exportCmd=events.find(e=>e.kind==='command'&&e.argv.includes('export'));
  assert.equal(importCmd.exit_code,0);assert.equal(exportCmd.exit_code,0);
  assert.deepEqual(JSON.parse(exportCmd.stdout).messages,fixture.transfer.messages);
  const reportMessage=fixture.transfer.messages.find(m=>m.id==='msg_vis38_report');
  if(entry.native_context?.reference_delivery){
    assert.equal(reportMessage,undefined);
    assert.equal(entry.native_context.prior_notifications.at(-1).payload,entry.prompts[0]?.body.parts[0].text??null);
    assert.equal(fixture.notification_text_sha256,entry.payload?sha(entry.payload):null);
  }else if(entry.prompts.length){assert.equal(reportMessage.text,entry.payload);assert.equal(fixture.notification_text_sha256,sha(entry.payload));}
  else assert.equal(reportMessage,undefined);
  const markers={};
  for(const needle of ['▣ DCP','│','▣ Compression','→ Topic:','→ Items:']){
    markers[needle]=[];
    for(const [y,row] of grid.cells.entries())for(let x=0;x<row.length;x++){
      const symbols=[...needle];
      if(symbols.every((s,i)=>row[x+i]?.symbol===s))markers[needle].push({x,y,cell:row[x],left_border:row[2],left_padding:row.slice(3,5),row_last_cell:row.at(-1)});
    }
  }
  let selectedHeader;
  if(entry.native_context?.display?.notification==='off'||entry.native_context?.display?.channel==='toast'||spec.case==='off')assert.equal(markers['▣ DCP'].length,0);
  else if(capture.stage==='top'||spec.case!=='long-topic-summary'){
    selectedHeader=markers['▣ DCP'][0];
    if(entry.native_context){
      const runNeedle='▣ Compression #'+entry.native_context.typed_snapshot.ordinal+' ';
      const matching=grid.cells.flatMap((row,y)=>row.map(c=>c.symbol).join('').includes(runNeedle)?[y]:[]);
      assert(matching.length<=1,'Selected actual run must not be duplicated');
      selectedHeader=markers['▣ DCP'].find(h=>h.y===matching[0]-3);
      // A scrolled full frame may clip the first line at the reserved tab row.
      // Other captured viewports retain the unmasked header and geometry.
    }else assert.equal(markers['▣ DCP'].length,1,'Visible header must be unique');
    if(selectedHeader){
      const {x,y}=selectedHeader;
      assert.equal(x,5,'Actual U34 border + two-cell padding geometry');
      assert.equal(grid.cells[y][2].symbol,'┃');
      assert.equal(grid.cells[y][3].symbol,' ');assert.equal(grid.cells[y][4].symbol,' ');
      assert.equal(grid.cells[y][3].bg,grid.cells[y][5].bg,'U34 raised background reaches padding');
      if(spec.case==='detailed'||entry.native_context?.display?.notification!=='minimal'){
         assert(markers['▣ Compression'].some(m=>m.y===y+3),'Header/blank/bar/run spacing');
         assert(markers['→ Topic:'].some(m=>m.y===y+4));
         assert(markers['→ Items:'].some(m=>m.y>=y+5&&m.y<=y+8),'Long actual topic may wrap before Items');
      }
    }
  }
  measurements.push({name:capture.name,stage:capture.stage,columns:grid.columns,rows:grid.rows,png:[render.png_width,render.png_height],cursor:grid.cursor,markers,
    selected_header_visible:!!selectedHeader});
}
for(const entry of goldens.notifications.filter(n=>n.native_context&&(!n.native_context.display||n.native_context.display.notification==='detailed'&&n.native_context.display.channel==='chat'))){
  assert(measurements.some(m=>m.name.startsWith(entry.id+'-')&&m.selected_header_visible),entry.id+' must have a captured complete header at one approved viewport');
}
const result={status:'PASS_REFERENCE_INTEGRITY_AND_U34_GEOMETRY',qualification:'Source-derived pinned display reference only; no native/parity PASS',notification_cases:goldens.notifications.length,bar_cases:goldens.bars.length,token_cases:goldens.tokens.length,captures:lock.captures.length,imports:lock.attempts.length,provider_requests:0,source_hashes:hashes,named_number_differences:goldens.tokens.filter(t=>t.named_difference),measurements};
fs.writeFileSync(report,JSON.stringify(result,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({status:result.status,captures:result.captures,imports:result.imports,report}));
