"""Fresh encoder with independent manual in-memory and durable byte/meta inverse."""
import ast
import gc
import importlib.util
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
HERE = Path(__file__).parent
spec = importlib.util.spec_from_file_location('wrap_observer', HERE/'vis36-wrap-check.py')
audit = importlib.util.module_from_spec(spec); spec.loader.exec_module(audit)
check, producer, OUT, TMP = audit.check, audit.producer, audit.OUT, audit.TMP
storage = audit.module('wrap_storage', audit.REPO/'scripts/archive_failed_capture.py')
old = HERE/'vis36-generic-summary-retain.py'
tree = audit.declarations(old, {'pack'})
for n in ast.walk(tree):
    if isinstance(n,ast.Constant) and n.value == 'vis36-generic-summary-': n.value = 'vis36-wrap-'
exec(compile(ast.fix_missing_locations(tree),str(old),'exec'))

def acquisition(profile):
    before = check.load(TMP/'vis36-wrap-acquisition.json')
    mapping = audit.roots(profile); producer.ROOTS = mapping
    files = producer.source_files(); producer.vis36_prompt_checks(files,profile,'008')
    inv = audit.inventory(mapping,files)
    assert all(inv[k] == before['profiles'][profile][k] for k in inv)
    pack(profile, inv, files)

def gates():
    paths = [*sorted(TMP.glob('vis37-form-gate-vis36-wrap-*.*')),
             *sorted(TMP.glob('vis36-wrap-*.py')), *sorted(TMP.glob('vis36-wrap-*.json')),
             TMP/'vis36-prompt-capture-r8.py', TMP/'vis37-form-gate-20261012.py',
             *sorted(HERE.glob('vis36-wrap-*.py'))]
    assert len(paths) == len(set(paths))
    mapping = {'gates':TMP, 'programs':HERE}
    files = {('gates/' if p.parent == TMP else 'programs/')+p.name:check.read_bytes(p) for p in paths}
    pack('gates', audit.inventory(mapping,files), files)

if __name__ == '__main__':
    assert len(sys.argv) == 2 and sys.argv[1] in ('wide','small','gates')
    if sys.argv[1] == 'gates': gates()
    else: acquisition(sys.argv[1])
