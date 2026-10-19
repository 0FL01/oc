// Narrow current-source runtime seal for the background Subagent qualification.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

export const backgroundCommit='bc5ebf00a4c169ece10f276e6ce02eaf18c326e6';
const digest=file=>createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const run=(argv,cwd,env)=>{
  const result=spawnSync(argv[0],argv.slice(1),{cwd,env,encoding:'utf8',maxBuffer:16*1024*1024});
  if(result.status!==0)throw Error('Reference command failed: '+JSON.stringify(argv)+'\n'+result.stderr);
  return result.stdout.trim();
};
const owned=name=>{
  const real=fs.realpathSync(name),root=path.join(os.homedir(),'.cache/opencode-tmp/opencode');
  if(!real.startsWith(root+path.sep)||fs.statSync(real).uid!==process.getuid()||process.getuid()===0)
    throw Error('Current reference inputs must be current-owned approved-cache paths');
  return real;
};
const names=donor=>run(['git','ls-files','-z'],donor).split('\0').filter(Boolean).sort();
export function checkBackgroundArchive(source,tracked) {
  const expected=new Set(tracked);
  const walk=directory=>{
    for(const entry of fs.readdirSync(directory,{withFileTypes:true})) {
      if(entry.name==='node_modules')continue; // Explicit provisioned dependency boundary.
      const file=path.join(directory,entry.name),relative=path.relative(source,file);
      if(entry.isDirectory()){walk(file);continue;}
      if(!expected.has(relative)&&relative!=='packages/cli/.cache/bun-app-archive.bin')throw Error('Unexpected current-reference source input: '+relative);
    }
  };
  walk(source);
}
const sources=(donor,source)=>{
  const tracked=names(donor);if(fs.realpathSync(source)!==fs.realpathSync(donor))checkBackgroundArchive(source,tracked);
  return Object.fromEntries(tracked.filter(name=>!name.split('/').some(part=>part==='.env'||part.startsWith('.env.'))).map(name=>[name,digest(path.join(source,name))]));
};
const same=(left,right)=>JSON.stringify(left)===JSON.stringify(right);
function donorCheck(donor) {
  if(run(['git','rev-parse','HEAD'],donor)!==backgroundCommit||run(['git','status','--porcelain','--untracked-files=no'],donor))
    throw Error('Current TS checkout identity/clean-source guard failed');
}
export function verifyBackgroundReference(manifestFile,binary,donor) {
  owned(manifestFile);owned(binary);
  const seal=JSON.parse(fs.readFileSync(manifestFile));
  donorCheck(donor);
  if(seal.schema!==2||seal.commit!==backgroundCommit||seal.build.exit_code!==0||!seal.build.argv.includes('--no-env-file')||seal.bun.version!=='1.4.2'||seal.opentui!=='0.5.17'||seal.version!=='2.0.31')
    throw Error('Unsupported current TS runtime provenance');
  for(const suffix of ['/libopentui.so','/watcher.node','/bin/opencode-pty'])if(!Object.keys(seal.runtime_inputs).some(file=>file.endsWith(suffix)))throw Error('Unsealed selected native input '+suffix);
  if(!same(seal.source,sources(donor,donor))||!same(seal.source,sources(donor,owned(seal.source_root))))
    throw Error('Current reference source changed since build');
  for(const [file,hash] of Object.entries(seal.runtime_inputs))if(digest(owned(file))!==hash)throw Error('Current reference runtime input changed: '+file);
  if(digest(binary)!==seal.binary_sha256||digest(owned(seal.bun.path))!==seal.bun.sha256)
    throw Error('Current reference executable/toolchain changed');
  return {manifest_sha256:digest(manifestFile),...seal};
}

// This explicitly rebuilds the already provisioned locked CLI in owned scratch;
// it never installs packages, modifies the donor or invokes dev:live/services.
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const [scratchArg,donorArg]=process.argv.slice(2);
  const scratch=owned(scratchArg),donor=path.resolve(donorArg),source=owned(path.join(scratch,'src'));
  donorCheck(donor);
  const sourceManifest=sources(donor,donor);
  if(!same(sourceManifest,sources(donor,source)))throw Error('Scratch is not the current source archive');
  const bun=owned(path.join(scratch,'toolchain/bun-linux-x64/bun'));
  const env={HOME:path.join(scratch,'install-home'),PATH:path.dirname(bun)+':/usr/bin:/bin',TMPDIR:path.join(scratch,'tmp'),LANG:'C.UTF-8',OPENCODE_CHANNEL:'local',OPENCODE_VERSION:'2.0.31'};
  const version=run([bun,'--version'],source,env);
  const packages=run([bun,'--no-env-file','-e',`const path=require('node:path');const owners={};for(const name of ['@opentui/core','@opentui/solid','solid-js','@parcel/watcher','@opencode-ai/pty']){const file=require.resolve(name+'/package.json',{paths:[process.argv[1]+'/packages/cli',process.argv[1]+'/packages/tui',process.argv[1]+'/packages/core']});owners[name]=file;console.log(JSON.stringify({name,file,version:require(file).version}));}for(const [owner,name] of [['@opentui/core','@opentui/core-linux-x64'],['@parcel/watcher','@parcel/watcher-linux-x64-glibc'],['@opencode-ai/pty','@opencode-ai/pty-linux-x64-gnu']]){const file=require.resolve(name+'/package.json',{paths:[path.dirname(owners[owner])]});console.log(JSON.stringify({name,file,version:require(file).version}));}`,source],source,env).split('\n').map(JSON.parse);
  if(version!=='1.4.2'||packages.some(p=>p.name.startsWith('@opentui/')&&p.version!=='0.5.17'||p.name==='solid-js'&&p.version!=='1.9.15'))throw Error('Current renderer/toolchain version mismatch');
  const inputs={};
  const walk=directory=>{for(const entry of fs.readdirSync(directory,{withFileTypes:true})){const file=path.join(directory,entry.name);if(entry.isDirectory())walk(file);else if(entry.isFile())inputs[fs.realpathSync(file)]=digest(file);}};
  for(const pkg of packages)walk(path.dirname(fs.realpathSync(pkg.file)));
  for(const suffix of ['/libopentui.so','/watcher.node','/bin/opencode-pty'])if(!Object.keys(inputs).some(file=>file.endsWith(suffix)))throw Error('Missing selected native build input '+suffix);
  const argv=[bun,'--no-env-file','run','--cwd',path.join(source,'packages/cli'),'script/build.ts','--single','--skip-install','--skip-web-ui','--outdir='+path.join(scratch,'dist')];
  const stdout=run(argv,source,env);
  const log=path.join(scratch,'sealed-build.log');fs.writeFileSync(log,stdout+'\n');
  donorCheck(donor);
  if(!same(sourceManifest,sources(donor,source)))throw Error('Source changed during reference build');
  const binary=owned(path.join(scratch,'dist/cli-linux-x64/bin/opencode'));
  const seal={schema:2,commit:backgroundCommit,version:'2.0.31',source_root:source,source:sourceManifest,
    bun:{path:bun,version,sha256:digest(bun)},opentui:'0.5.17',packages,runtime_inputs:inputs,
    build:{argv,env,exit_code:0,stdout_sha256:digest(log)},binary_sha256:digest(binary)};
  const manifest=path.join(scratch,'background-reference.json');fs.writeFileSync(manifest,JSON.stringify(seal,null,2)+'\n');
  verifyBackgroundReference(manifest,binary,donor);
  console.log(manifest);
}
