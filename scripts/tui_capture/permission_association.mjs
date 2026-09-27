#!/usr/bin/env node
// Read-only association audit of the bounded campaign and current Rust inputs.
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
const base=path.resolve(process.argv[2]),output=path.resolve(process.argv[3]);
const first=Number(process.argv[4]),last=Number(process.argv[5]);
if(fs.existsSync(output))throw Error('Immutable output exists');
const read=p=>JSON.parse(fs.readFileSync(p));
const digest=b=>createHash('sha256').update(b).digest('hex');
const canonical=value=>JSON.stringify(value,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v);
const rust=p=>p.startsWith('crates/')||p.startsWith('Cargo')||p==='rust-toolchain.toml';
const runs=[];
for(let i=first;i<=last;i++){
 const name=`permission20260927-${String(i).padStart(2,'0')}`,dir=path.join(base,name);
 const lock=read(path.join(dir,'capture.lock.json')),manifest=read(path.join(dir,'source-manifest.json'));
 const inputs=Object.fromEntries(Object.entries(manifest).filter(([p])=>rust(p)));
 const current_mismatches=Object.entries(inputs).filter(([p,h])=>!fs.existsSync(p)||digest(fs.readFileSync(p))!==h).map(([p])=>p);
 runs.push({name,captured_head:lock.oc.commit,source_manifest_sha256:lock.oc.source_manifest_sha256,manifest_hash_valid:digest(canonical(manifest))===lock.oc.source_manifest_sha256,rust_inputs_sha256:digest(JSON.stringify(inputs)),binary_sha256:lock.oc.executable_sha256,build:read(path.join(dir,'commands.json')).filter(c=>c.argv[0]==='cargo').map(c=>({argv:c.argv,exit_code:c.exit_code})),current_mismatches});
}
const current_head=spawnSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).stdout.trim();
const current_binary_sha256=digest(fs.readFileSync('target/debug/oc'));
const report={current_head,current_binary_sha256,runs,scope:'Only these source-built captures; no older evidence qualifies this source.'};
fs.writeFileSync(output,JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify(report,null,2));
if(runs.some(r=>!r.manifest_hash_valid||r.build.length!==1||r.build[0].exit_code!==0))process.exitCode=1;
