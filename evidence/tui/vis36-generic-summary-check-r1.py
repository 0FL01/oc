"""Read-only delivery audit, distinguishing acquisition and ordinary-build ELFs.

Only immutable pure declarations are reused via AST; no old module/HEAD guard or
entrypoint executes. Existing producer receipts and codec proofs stay immutable.
"""
import ast
import gc
import importlib.util
import json
import os
from pathlib import Path
import re
import resource
import sys
import types

sys.dont_write_bytecode=True
resource.setrlimit(resource.RLIMIT_AS,(384*1024**2,)*2)
REPO=Path('/home/opencode/ai/oc')
TMP=Path('/home/opencode/.cache/opencode-tmp/opencode')
OUT=REPO/'evidence/tui'
BASE='f2f2025e060bba9ab2d41932b888d344a88ee88b'
BEFORE=TMP/'vis36-generic-summary-before.json'
PROFILES=('wide','small')
R0=OUT/'vis36-generic-summary-check.py'
tree=ast.parse(R0.read_bytes())
node=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='declarations')
exec(compile(ast.fix_missing_locations(ast.Module(body=[node],type_ignores=[])),str(R0),'exec'))
check=types.ModuleType('generic_summary_r1_inverse')
old=TMP/'vis37-independent-20261012-sole-verifier/inverse_verifier.py'
names={'require','sha','digest','read_bytes','load','canonical_sha','metadata','git','producer_sha','proc_memory','quota',
       'vli','inverse_xz','cstring','octal','make_header','path_ok','inverse_ustar','compare_grid','decode_png','compare_png','elf'}
exec(compile(declarations(old,names,('SCOPE','PREFIXES'),True),str(old),'exec'),check.__dict__)
check.AS,check.FILE,check.TOTAL,check.OWNED=384*1024**2,16*1024**2,128*1024**2,1024**3
check.MEMBERS,check.XZ_DICT,check.XZ_DECODE=2000,16*1024**2,32*1024**2
check.REPO,check.TEMP=REPO,TMP
check.PREFIXES=(*check.PREFIXES,'vis36-')
check.ROOTS={}
check.DONOR='2670273ff17da96f85c5826ced57aa1b368754fa'
check.HEAD=BASE
check.FIELDS={'symbol','fg','bg','modifiers','width'}
names={'module','current_source','record','save','preservation','roots','inventory','inverse','whole_frames','gates'}
exec(compile(declarations(R0,names),str(R0),'exec'))
sys.path.insert(0,str(REPO/'scripts'))
producer=module('generic_summary_r1_strict_encoder',REPO/'scripts/retain_syntax_proof.py')
effects_ns=dict(json=json,collections=check.collections)
exec(compile(declarations(OUT/'vis36-prompt-audit.py',{'effects'},('STAGES',)),'effects_declarations','exec'),effects_ns)
STAGES=effects_ns['STAGES']
EXPECTED={str(REPO/'target/debug/oc'):'e1c86b37619525baa9e1b56b05e3890d49d89bd041ef7a7ef88638c1eeb5dfb9',
          str(REPO/'target/release/oc'):'dbac351c3b647dc376fc4d685ddf85c9babb8460229101d0c9ad289211b75ff9',
          str(TMP/'t44-reference/package/bin/opencode'):'2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a'}

def audit(delivery=False):
    assert os.getuid()==1003
    before=check.load(BEFORE);preservation(before)
    acquired=check.load(TMP/'vis36-generic-summary-acquisition-007-before.json')
    assert current_source()==acquired['source'] and acquired['source_sha256']=='e8dc500cd1d1b74b8499a62152a58b9846d764b7eb361c0828b8813be554675e'
    actual={p:check.elf(Path(p)) for p in EXPECTED}
    assert {p:r['sha256'] for p,r in actual.items()}==EXPECTED
    for p in actual:
        if '/target/release/' not in p:assert actual[p]==acquired['ELF'][p]
    for label,argv in (('ordinary-debug-help',['target/debug/oc','--help']),('ordinary-release-help',['target/release/oc','--help'])):
        gate=check.load(TMP/('vis37-form-gate-vis36-generic-summary-'+label+'.json'))
        assert gate['command']==argv and gate['exit_code']==0
    inverses={}
    for role in ('wide','small','wide-007','small-007','gates','gates-r1'):
        name='vis36-generic-summary-'+role
        inv=check.load(OUT/(name+'.inventory.json'))
        proof=inverse(OUT/(name+'.tar.xz'),inv)
        assert all(inv[k]==proof[k] for k in ('archive_sha256','archive_bytes','tar_sha256','tar_bytes','xz'))
        assert inv['inventory']==proof['tar']['inventory']
        inverses[role]=proof
    frames={p:whole_frames(p) for p in PROFILES}
    for profile in PROFILES:
        producer.ROOTS=roots(profile);files=producer.source_files()
        assert producer.vis36_prompt_checks(files,profile,'007')==acquired['profiles'][profile]['strict_producer_checks']
        assert {s:effects_ns['effects'](roots(profile)['current'],s) for s in ('oc','upstream')}==acquired['profiles'][profile]['effects']
        del files;gc.collect()
    gatefacts=gates();assert gatefacts['full_workspace_status']==gatefacts['affected_crate_status']=='PASS'
    state=check.load(REPO/'progress/STATE.json')
    if delivery:
        assert state['current']=='T44' and state['tasks']['T44']['status']=='active' and state['tasks']['T44']['last_seq']==144
        assert state['schema_version']==before['state']['schema_version']
        assert {k:v for k,v in state['tasks'].items() if k!='T44'}=={k:v for k,v in before['state']['tasks'].items() if k!='T44'}
        assert check.git('branch','--show-current').decode().strip()=='agent/oc-rust-port'
        assert check.git('remote','get-url','origin').decode().strip()=='git@github.com:0FL01/oc.git'
    result=dict(status='CHECKED_GENERIC_SUMMARY_SLICE_NOT_T44_PASS',source_sha256=acquired['source_sha256'],source_count=acquired['source_count'],
        acquisition_ELF=acquired['ELF'],ordinary_ELF=actual,profiles=frames,effects={p:v['effects'] for p,v in acquired['profiles'].items()},
        gates=gatefacts,inverses=inverses,prior_proofs=len(before['prior']),legacy_original_files=len(before['original_files']),
        legacy_original_directories=len(before['original_dirs']),quota=check.quota(),memory=check.proc_memory(),state=state)
    print(json.dumps(dict(status=result['status'],source_sha256=result['source_sha256'],ELF=EXPECTED,
        profiles={p:{k:v for k,v in f.items() if k not in ('frames','summary_rows')} for p,f in frames.items()},quota=result['quota'])))
    return result

if __name__=='__main__':
    assert len(sys.argv)==3 and sys.argv[1] in ('audit','delivery')
    save(Path(sys.argv[2]),audit(sys.argv[1]=='delivery'))
