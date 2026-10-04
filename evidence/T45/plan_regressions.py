#!/usr/bin/env python3
"""Risk-directed delivered native fixtures on the same retained normal ELFs."""
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'evidence/T50'))
import native_file_mutations as files
import native_live_model_switch as switching
import native_question as question
import native_session_move as move

def main():
    binaries = [Path(arg).resolve() for arg in sys.argv[1:]]
    hashes = {str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries}
    print(json.dumps({'before':hashes}),flush=True)
    for binary in binaries:
        for case in ('write','edit','patch','ask_stale','absolute_outside','symlink','child_deny'):
            print(json.dumps({'binary':str(binary),'pack':'T50-mutation',**files.run(binary,case)}),flush=True)
        for case in ('stream','variant','ask'):
            print(json.dumps({'binary':str(binary),'pack':'T50-live-switch',**switching.run(binary,case)}),flush=True)
        question.run(binary,'headless_auto',auto=True)
        question.run(binary,'general_ceiling',child=True)
        question.run(binary,'child_origin_consumer',child='helper',keys=b'1',expected=[['Native']])
        # Explicit-root placement plus trust/profile/preimage refusals target
        # this slice's roots/selection seam. Historical boundary's single-job
        # consumer predates the delivered foreground-shell durable rows.
        for case in ('explicit','untrusted','bad_agent','ask_stale'):
            print(json.dumps({'binary':str(binary),'pack':'Move',**move.run(binary,case)}),flush=True)
    after = {str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries}
    assert after==hashes
    print(json.dumps({'after':after,'cases_per_binary':17,'cleanup':'all exact owners joined before TempDir cleanup'}),flush=True)

if __name__=='__main__': main()
