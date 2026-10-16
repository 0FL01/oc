"""Exclusive final revision bundle; every prior helper/bundle stays immutable."""
import gc
import importlib.util
import json
from pathlib import Path
import sys

sys.dont_write_bytecode=True
HERE=Path(__file__).parent
spec=importlib.util.spec_from_file_location('generic_summary_r2',HERE/'vis36-generic-summary-check-r2.py')
audit=importlib.util.module_from_spec(spec);spec.loader.exec_module(audit)
check,producer,OUT,TMP=audit.check,audit.producer,audit.OUT,audit.TMP
storage=audit.module('generic_summary_r2_storage',audit.REPO/'scripts/archive_failed_capture.py')
old=HERE/'vis36-generic-summary-retain.py'
exec(compile(audit.declarations(old,{'pack'}),str(old),'exec'))
assert sys.argv[1:]==['gates']
paths=[*sorted(TMP.glob('vis37-form-gate-vis36-generic-summary-*.*')),
       *sorted(TMP.glob('vis36-generic-summary-*.py')),*sorted(TMP.glob('vis36-generic-summary-*.json')),
       TMP/'vis36-prompt-capture-r6.py',TMP/'vis36-prompt-capture-r7.py',TMP/'vis37-form-gate-20261012.py',
       *sorted(HERE.glob('vis36-generic-summary-*.py'))]
assert len(paths)==len(set(paths))
mapping={'gates':TMP,'programs':HERE}
files={('gates/' if p.parent==TMP else 'programs/')+p.name:check.read_bytes(p) for p in paths}
pack('gates-r2',audit.inventory(mapping,files),files)
