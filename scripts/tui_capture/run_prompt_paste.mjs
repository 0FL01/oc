#!/usr/bin/env node
// Serial fresh-source VIS07 diagnostic campaign; never share Cargo with another run.
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const here=path.dirname(fileURLToPath(import.meta.url));
const repo=path.resolve(here,'../..');
const root=path.resolve(process.argv[2]);
const reference='/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode';
fs.mkdirSync(root); // Immutable campaign: fail if any earlier attempt exists.
const runs=[];
for(const width of [120,79,80,121])for(const kind of ['chip','extra'])runs.push({name:kind+'-'+width,width,extra:kind==='extra'});
runs.splice(2,0,{name:'repeat-nav-120',width:120,navigation:true});
runs.splice(3,0,{name:'suffix-space-120',width:120,navigation:true,suffixSpace:true});
const result={started:new Date().toISOString(),scope:'Serial local fake-provider captures, fresh cargo build per run',runs:[]};
for(const run of runs) {
  const cargo=spawnSync('pgrep',['-a','-u',String(process.getuid()),'cargo|rustc'],{encoding:'utf8'});
  if(cargo.status!==1){result.blocker={before_run:run.name,reason:'Cargo exclusive preflight failed',processes:cargo.stdout,stderr:cargo.stderr};break;}
  const argv=[path.join(here,'capture.mjs'),'--leader-pending','true','--leader-config','default',
    ...(run.extra?['--leader-extra','true']:[]),...(run.navigation?['--leader-paste-navigation','true']:[]),...(run.suffixSpace?['--leader-paste-suffix-space','true']:[]),
    '--geometry','true','--sidebar','hide','--sample','short','--columns',String(run.width),'--rows','40',
    '--reference',reference,'--oc',path.join(repo,'target/debug/oc'),'--build-oc','true','--output',path.join(root,run.name)];
  const child=spawnSync(process.execPath,argv,{cwd:repo,encoding:'utf8',env:{...process.env,CARGO_BUILD_JOBS:'3',TMPDIR:'/home/opencode/.cache/opencode-tmp/opencode'},timeout:180000,maxBuffer:16*1024*1024});
  fs.writeFileSync(path.join(root,run.name+'.runner.log'),(child.stdout||'')+(child.stderr||''),{flag:'wx'});
  result.runs.push({...run,argv:[process.execPath,...argv],exit_code:child.status,error:child.error?.message,signal:child.signal});
  console.log(JSON.stringify({run:run.name,exit_code:child.status,error:child.error?.message}));
  // Expected frame differences are exit 1; unknown effects/blockers stop continuation.
  if(child.status!==0&&child.status!==1)break;
}
result.finished=new Date().toISOString();
fs.writeFileSync(path.join(root,'campaign.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});
process.exitCode=result.runs.length===runs.length&&result.runs.every(r=>r.exit_code===0||r.exit_code===1)?0:2;
