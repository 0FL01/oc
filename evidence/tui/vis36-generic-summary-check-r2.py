"""Final independent read-only audit; exact workspace-test command selection."""
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
check=types.ModuleType('generic_summary_r2_inverse')
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
selected=declarations(R0,names)
replaced=0
for n in ast.walk(selected):
    if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='workspace' for t in n.targets):
        assert ast.unparse(n.value)=="{k: v for k, v in records.items() if 'workspace' in k}"
        n.value=ast.parse("{k:v for k,v in records.items() if 'test' in v['command'] and '--workspace' in v['command']}",mode='eval').body
        replaced+=1
assert replaced==1
exec(compile(ast.fix_missing_locations(selected),str(R0),'exec'))
sys.path.insert(0,str(REPO/'scripts'))
producer=module('generic_summary_r2_strict_encoder',REPO/'scripts/retain_syntax_proof.py')
effects_ns=dict(json=json,collections=check.collections)
exec(compile(declarations(OUT/'vis36-prompt-audit.py',{'effects'},('STAGES',)),'effects_declarations','exec'),effects_ns)
STAGES=effects_ns['STAGES']
R1=OUT/'vis36-generic-summary-check-r1.py'
selected=declarations(R1,{'audit'},('EXPECTED',))
replaced=0
for n in ast.walk(selected):
    if isinstance(n,ast.Constant) and n.value==('wide','small','wide-007','small-007','gates','gates-r1'):
        raise AssertionError('unexpected constant tuple AST')
    if isinstance(n,ast.Tuple) and all(isinstance(e,ast.Constant) for e in n.elts) and tuple(e.value for e in n.elts)==('wide','small','wide-007','small-007','gates','gates-r1'):
        n.elts.append(ast.Constant('gates-r2'));replaced+=1
assert replaced==1
exec(compile(ast.fix_missing_locations(selected),str(R1),'exec'))

if __name__=='__main__':
    assert len(sys.argv)==3 and sys.argv[1] in ('audit','delivery')
    save(Path(sys.argv[2]),audit(sys.argv[1]=='delivery'))
