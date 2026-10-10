// Lossless storage for complete, freshly observed frames. One owner per side/run.
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {gzipSync} from 'node:zlib';

export function frameArtifacts(dir, compact) {
  const pngs=new Map();
  return (stem,extension,value)=>{
    if(path.basename(stem)!==stem)throw Error('Frame artifact must belong to this capture side');
    const plain=Buffer.isBuffer(value)?value:Buffer.from(value);
    const gzip=compact&&['.render.json','.trace.json','.txt','.vt'].includes(extension);
    const bytes=gzip?gzipSync(plain):plain;
    const file=stem+extension+(gzip?'.gz':''), target=path.join(dir,file);
    if(fs.existsSync(target))throw Error('Refusing to overwrite frame artifact '+file);
    const hash=createHash('sha256').update(bytes).digest('hex');
    const prior=compact&&extension==='.png'?pngs.get(hash):undefined;
    if(prior) {
      if(!fs.readFileSync(prior).equals(bytes))throw Error('Frame artifact hash collision');
      fs.linkSync(prior,target);
    } else {
      fs.writeFileSync(target,bytes,{flag:'wx'});
      if(compact&&extension==='.png')pngs.set(hash,target);
    }
    return {file,sha256:hash};
  };
}
