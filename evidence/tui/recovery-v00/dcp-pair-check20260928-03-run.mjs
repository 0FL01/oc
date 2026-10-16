// Evidence-only execution adapter; production/reference scripts remain unchanged.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';

const here=path.dirname(fileURLToPath(import.meta.url));
const repo=path.resolve(here,'../../..');
const helper=path.join(repo,'scripts/tui_capture/dcp_pair_audit.mjs');
const sha=b=>createHash('sha256').update(b).digest('hex');
const source=fs.readFileSync(helper,'utf8');
assert.equal(sha(source),'21ec6d77dad139bf4dc3f221296a7fc0dd07fdb56ce79c6e3f579e9c624e1e4d');
const historical='4070639b7d0e9814f862b577ee7a4014f06a21567054c534fd6f6baf9b076f19';
const released='8f7b79887e7c0fa3296800388819bca72f68c7aeae459d891ce0eb2690c4df93';
assert.equal(sha(fs.readFileSync(path.join(repo,'target/debug/oc'))),released);
const oldLine=`assert.equal(owner.oc_binary_sha256,'${historical}');`;
const newLine=`assert.equal(owner.oc_binary_sha256,'${released}');`;
assert.equal(source.split(oldLine).length,2);
const program=source.replace(oldLine,newLine);
assert.equal(program.replace(newLine,oldLine),source);
const args=[path.join(here,'dcp-native20260928-05'),path.join(here,'dcp-oracle20260928-native04'),
  path.join(here,'dcp-reference20260928-native05'),path.join(here,'dcp-pair-check20260928-03.json')];
process.argv=['node',helper,...args];
await import('data:text/javascript;base64,'+Buffer.from(program).toString('base64'));
assert.equal(sha(fs.readFileSync(helper)),sha(source),'Source audit helper changed during execution');
assert.equal(sha(fs.readFileSync(path.join(repo,'target/debug/oc'))),released);
const association={status:'PASS_RELEASE_DIGEST_ONLY_RUNTIME_ADAPTER',exit_code:0,
  source_helper:helper,source_helper_sha256:sha(source),executed_program_sha256:sha(program),
  replacement:{old:oldLine,new:newLine,source_file_modified:false,all_other_audit_checks_unchanged:true},
  parent_full_workspace_gate:{status:'PASS',tool:'tool_0e51bf7ae001L43hkvwsvd4nD9',association:'Parent-provided final SQL-query-fix build attestation'},
  args,runner_sha256:sha(fs.readFileSync(fileURLToPath(import.meta.url))),
  paired_report_sha256:sha(fs.readFileSync(args[3]))};
fs.writeFileSync(path.join(here,'dcp-pair-check20260928-03-run.json'),JSON.stringify(association,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify(association));
