// Storage-format test, not a replacement renderer or application-frame oracle.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {frameArtifacts} from './frame_artifacts.mjs';

const root=fs.mkdtempSync(path.join('/home/opencode/.cache/opencode-tmp/opencode','frame-storage-'));
try {
  const dirs=['oc','upstream','plain'].map(side=>path.join(root,side));
  for(const dir of dirs)fs.mkdirSync(dir);
  const [oc,upstream,plain]=dirs.map((dir,index)=>frameArtifacts(dir,index<2));
  const full=Buffer.from('full frame Ω界\u001b[31m\n'+JSON.stringify({cells:[{symbol:'界',fg:'#eeeeee',bg:'#0a0a0a',modifiers:['bold']}],cursor:{x:2,y:1,visible:true}}));
  for(const extension of ['.render.json','.trace.json','.txt','.vt']) {
    const sealed=oc('complete',extension,full), stored=fs.readFileSync(path.join(dirs[0],sealed.file));
    assert.deepEqual(gunzipSync(stored),full);
    assert.equal(createHash('sha256').update(stored).digest('hex'),sealed.sha256);
    assert.throws(()=>oc('complete',extension,full),/overwrite/);
    const legacy=plain('complete',extension,full);
    assert.equal(legacy.file,'complete'+extension);
    assert.deepEqual(fs.readFileSync(path.join(dirs[2],legacy.file)),full);
  }
  const raster=Buffer.from('independently observed identical raster bytes');
  for(const writer of [oc,upstream])for(const name of ['first','second'])writer(name,'.png',raster);
  assert.equal(fs.statSync(path.join(dirs[0],'first.png')).ino,fs.statSync(path.join(dirs[0],'second.png')).ino);
  assert.notEqual(fs.statSync(path.join(dirs[0],'first.png')).ino,fs.statSync(path.join(dirs[1],'first.png')).ino);
  assert.deepEqual(fs.readFileSync(path.join(dirs[0],'second.png')),raster);
  assert.throws(()=>oc('../escape','.png',raster),/belong/);
  console.log('PASS: complete lossless sidecars, exact encoded seals, immutable paths and same-side-only byte-identical PNG dedup');
} finally {fs.rmSync(root,{recursive:true});}
