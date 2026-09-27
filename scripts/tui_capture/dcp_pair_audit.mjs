#!/usr/bin/env node
// Full, unmasked VIS38 paired evidence audit: real native held stages vs U34.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';

const [nativeArg, oracleArg, referenceArg, reportArg]=process.argv.slice(2);
assert(nativeArg&&oracleArg&&referenceArg&&reportArg,'NATIVE_OWNER_DIR NATIVE_ORACLE_DIR PINNED_REFERENCE_DIR NEW_REPORT_JSON');
const sha=b=>createHash('sha256').update(b).digest('hex');
const read=p=>JSON.parse(fs.readFileSync(p));
const native=path.resolve(nativeArg),oracle=path.resolve(oracleArg),reference=path.resolve(referenceArg),report=path.resolve(reportArg);
assert.equal(path.dirname(report),path.dirname(reference));
assert.match(path.basename(report),/^dcp-pair-check[a-z0-9-]+\.json$/);
const owner=read(path.join(native,'result.json'));
assert.equal(owner.status,'PASS_ACTUAL_SINGLE_MULTI_RESTART_OWNER_PROBE');
assert.equal(owner.oc_binary_sha256,'4070639b7d0e9814f862b577ee7a4014f06a21567054c534fd6f6baf9b076f19');
for(const [file,hash] of Object.entries(owner.evidence_seals))assert.equal(sha(fs.readFileSync(path.join(native,file))),hash,'sealed owner evidence '+file);
const association=read(path.join(oracle,'owner-association.json'));
assert.equal(sha(fs.readFileSync(path.join(native,'result.json'))),association.result_sha256);
const goldens=read(path.join(oracle,'goldens.json'));
assert.equal(goldens.notifications.length,8);
const refLock=read(path.join(reference,'capture.lock.json'));
assert.equal(refLock.goldens_sha256,sha(fs.readFileSync(path.join(oracle,'goldens.json'))));
assert.equal(refLock.attempts.length,24);
const cells=file=>read(file).cells;
const text=rows=>rows.map(row=>row.map(c=>c.symbol).join('')).join('\n');
const passes=[],files=[];
let fullCells=0,unequalCells=0,textUnequal=0,fgUnequal=0,bgUnequal=0,styleUnequal=0,cursorUnequal=0;
for(const entry of goldens.notifications){
  const stage=entry.native_context.stage;
  const nativeCase=owner.display_cases.find(c=>c.stage===stage);
  assert(nativeCase,'Missing actual owner stage '+stage);
  assert.deepEqual(nativeCase.typed_snapshot,entry.native_context.typed_snapshot);
  const evidence=read(path.join(native,'native-capture-'+stage+'.json'));
  assert.equal(evidence.provider_requests_before,evidence.provider_requests_after);
  assert.equal(evidence.exit_code,0);
  const nativeDir=path.resolve(evidence.output),nativeLock=read(path.join(nativeDir,'capture.lock.json'));
  assert.equal(sha(fs.readFileSync(path.join(nativeDir,'capture.lock.json'))),evidence.lock_sha256);
  assert.equal(nativeLock.binary.sha256,owner.oc_binary_sha256);
  assert.equal(nativeLock.attempts.length,3);
  assert(nativeLock.attempts.every(a=>a.status==='PASS_NATIVE_STAGE_CAPTURE'));
  for(const attempt of nativeLock.attempts){
    const protocol=read(path.join(nativeDir,attempt.name,'protocol.json'));
    const launch=protocol.find(e=>e.kind==='launch');
    assert.equal(launch.binary_sha256,owner.oc_binary_sha256);
    assert.equal(launch.native_spec.session,entry.native_context.session);
    assert.equal(launch.native_spec.operation_id,entry.native_context.operation_id);
    assert.equal(launch.native_spec.context_snapshot,nativeCase.context_snapshot);
    assert(protocol.some(e=>e.kind==='exit'&&e.code===0&&e.termination==='natural'));
    assert(!protocol.some(e=>e.kind==='unexpected_provider_request'));
  }
  for(const [cols,rows] of [[80,24],[120,40],[160,48]]){
    const name=`native-${stage}-${cols}x${rows}`;
    const nr=path.join(nativeDir,name),rr=path.join(reference,name);
    const ns=read(path.join(nr,'bottom.cells.json')),rs=read(path.join(rr,'bottom.cells.json'));
    assert.equal(ns.origin,'oc');assert.equal(rs.origin,'upstream');
    assert.equal(ns.columns,cols);assert.equal(rs.columns,cols);
    assert.equal(ns.rows,rows);assert.equal(rs.rows,rows);
    const nn=ns.cells,rn=rs.cells;
    assert.equal(nn.length,rows);assert.equal(rn.length,rows);
    const counts={cells:cols*rows,unequal:0,text:0,foreground:0,background:0,modifiers:0};
    for(let y=0;y<rows;y++){
      assert.equal(nn[y].length,cols);assert.equal(rn[y].length,cols);
      for(let x=0;x<cols;x++){
        const a=nn[y][x],b=rn[y][x];
        if(JSON.stringify(a)!==JSON.stringify(b))counts.unequal++;
        if(a.symbol!==b.symbol)counts.text++;
        if(a.fg!==b.fg)counts.foreground++;
        if(a.bg!==b.bg)counts.background++;
        if(JSON.stringify(a.modifiers)!==JSON.stringify(b.modifiers))counts.modifiers++;
      }
    }
    const diffCursor=JSON.stringify(ns.cursor)!==JSON.stringify(rs.cursor);
    const nativeText=text(nn),referenceText=text(rn);
    const runNeedle='▣ Compression #'+entry.native_context.typed_snapshot.ordinal+' ';
    const visible={native_header:nativeText.includes('▣ DCP |'),reference_header:referenceText.includes('▣ DCP |'),
      native_run:nativeText.includes(runNeedle),reference_run:referenceText.includes(runNeedle)};
    const option=entry.native_context.display;
    if(option?.notification==='off'||option?.channel==='toast'){
      assert(!nativeText.includes('▣ DCP |')&&!referenceText.includes('▣ DCP |'),name+' hidden chat card');
    }
    if(option?.notification==='minimal'){
      assert(nativeText.includes('— Compression #3')&&referenceText.includes('— Compression #3'),name+' minimal');
    }
    let focusedCard=null;
    if(cols===120&&(!option||option.notification==='detailed'&&option.channel==='chat')){
      const [header,,bar,runLine]=entry.payload.split('\n');
      focusedCard={expected_header:header,expected_run:runLine,expected_bar:bar,
        native_header:nativeText.includes(header),reference_header:referenceText.includes(header),
        native_run:nativeText.includes(runLine),reference_run:referenceText.includes(runLine),
        native_bar:nativeText.includes(bar),reference_bar:referenceText.includes(bar)};
      assert(focusedCard.native_header&&focusedCard.reference_header,name+' cumulative header');
      assert(focusedCard.native_run&&focusedCard.reference_run,name+' run line');
      assert(focusedCard.native_bar&&focusedCard.reference_bar,name+' frozen categorical bar');
      const summaryVisible=entry.payload.includes('→ Compression (~');
      assert.equal(nativeText.includes('→ Compression (~'),summaryVisible);
      assert.equal(referenceText.includes('→ Compression (~'),summaryVisible);
      focusedCard.summary_visible=summaryVisible;
    }
    const ncap=nativeLock.captures.find(c=>c.name===name&&c.stage==='bottom');
    const rcap=refLock.captures.find(c=>c.name===name&&c.stage==='bottom');
    assert(ncap&&rcap);
    for(const [dir,cap] of [[nr,ncap],[rr,rcap]])for(const [ext,hash] of Object.entries(cap.files)){
      const filepath=path.join(dir,'bottom.'+ext);
      assert.equal(sha(fs.readFileSync(filepath)),hash,name+' '+ext+' sealed full capture');
      files.push({name,origin:dir===nr?'native':'original',file:ext,sha256:hash,bytes:fs.statSync(filepath).size});
    }
    const npng=fs.readFileSync(path.join(nr,'bottom.png')),rpng=fs.readFileSync(path.join(rr,'bottom.png'));
    const dimensions={native:[npng.readUInt32BE(16),npng.readUInt32BE(20)],reference:[rpng.readUInt32BE(16),rpng.readUInt32BE(20)]};
    assert.deepEqual(dimensions.native,dimensions.reference);
    passes.push({name,stage,size:[cols,rows],full_grid:counts,full_png:{native_sha256:sha(npng),reference_sha256:sha(rpng),equal:sha(npng)===sha(rpng),dimensions},
      cursor:{native:ns.cursor,reference:rs.cursor,equal:!diffCursor},visible,focused_card:focusedCard,
      native_frames:nativeLock.captures.filter(c=>c.name===name).map(c=>c.stage),
      reference_frames:refLock.captures.filter(c=>c.name===name).map(c=>c.stage),
      title_time_path_footer_are_unmasked:true,delivery:'Actual native committed tool card vs unchanged D05 payload imported into original U34'});
    fullCells+=counts.cells;unequalCells+=counts.unequal;textUnequal+=counts.text;fgUnequal+=counts.foreground;bgUnequal+=counts.background;styleUnequal+=counts.modifiers;cursorUnequal+=Number(diffCursor);
  }
}
const result={status:'PASS_PAIRED_EVIDENCE_INTEGRITY_WITH_DIFFERENCES',qualification:'Actual owner and original captures verified; equality only when per-frame counts are zero; no VIS38 visual parity PASS asserted',
  binary_sha256:owner.oc_binary_sha256,source_HEAD:owner.source_HEAD,source_oracle_sha256:sha(fs.readFileSync(path.join(oracle,'goldens.json'))),
  cases:passes.length,owner_operations:owner.checks.filter(c=>c.typed_snapshot).length,
  owner_provider_requests:owner.provider_requests,display_provider_requests:0,full_bottom_cells:fullCells,
  unequal_bottom_cells:unequalCells,unequal_text_cells:textUnequal,unequal_foreground_cells:fgUnequal,
  unequal_background_cells:bgUnequal,unequal_modifier_cells:styleUnequal,unequal_cursor_frames:cursorUnequal,
  bottom_png_equal:passes.filter(p=>p.full_png.equal).length,case_results:passes,
  file_seals:files,limitations:['Full frame metrics include truthful original/native timings, session title, isolated cwd, native runtime footer and original agent hints','Original public import maps intercepted D05 ignored user text into U34, not a legacy JS plugin compatibility or live native toast proof','Bottom frames of long cards at 80x24 may clip headers; immutable unmasked scrolled frames are retained separately','No claim of failure/cancel/no-gain, public DTO, Undo/Redo or resource gate']};
fs.writeFileSync(report,JSON.stringify(result,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({status:result.status,cases:result.cases,owner_operations:result.owner_operations,full_bottom_cells:fullCells,unequal_bottom_cells:unequalCells,unequal_text_cells:textUnequal,unequal_cursor_frames:cursorUnequal,bottom_png_equal:result.bottom_png_equal,report}));
