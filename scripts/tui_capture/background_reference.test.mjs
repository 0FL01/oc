import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {checkBackgroundArchive} from './background_reference.mjs';

test('reference archive rejects extra source/dotenv inputs but admits only declared build outputs/dependencies',()=>{
  const root=fs.mkdtempSync(path.join(os.homedir(),'.cache/opencode-tmp/opencode/background-seal-test-'));
  try {
    fs.mkdirSync(path.join(root,'packages/cli/.cache'),{recursive:true});
    fs.mkdirSync(path.join(root,'node_modules'),{recursive:true});
    fs.writeFileSync(path.join(root,'package.json'),'{}');
    fs.writeFileSync(path.join(root,'packages/cli/.cache/bun-app-archive.bin'),'generated');
    fs.writeFileSync(path.join(root,'node_modules/provisioned'),'dependency');
    checkBackgroundArchive(root,['package.json']);
    for(const extra of ['.env','packages/untracked.ts']) {
      fs.writeFileSync(path.join(root,extra),'');
      assert.throws(()=>checkBackgroundArchive(root,['package.json']),/Unexpected current-reference source input/);
      fs.unlinkSync(path.join(root,extra));
    }
  } finally {fs.rmSync(root,{recursive:true});}
});

test('background profile rejects unbound native binary and unapproved code before loading tooling',()=>{
  const repo=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
  const args=['scripts/tui_capture/capture.mjs','--background-subagents','true','--background-animation','false','--background-reference','/not-read','--subagent-cards','true','--combined-composer','true','--tool-preview','true','--reference','/not-read','--build-oc','true','--tools','/usr/bin','--geometry','true','--sample','short','--sidebar','hide','--columns','120','--rows','40'];
  for(const [binary,error] of [['/not-current/oc2',/requires this worktree target\/debug\/oc2/],[path.join(repo,'target/debug/oc2'),/Background tools\/TMPDIR\/fonts must be owned approved-cache inputs/]]) {
    const result=spawnSync(process.execPath,[...args,'--oc',binary],{cwd:repo,encoding:'utf8',timeout:10000,maxBuffer:1024*1024});
    assert.notEqual(result.status,0);assert.match(result.stderr,error);
    assert.doesNotMatch(result.stderr,/Cannot find module 'playwright'/);
  }
});
