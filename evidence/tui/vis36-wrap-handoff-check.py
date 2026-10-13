"""Read-only historical acquisition inverse after unexpected concurrent source writes.

Does not qualify current source/ELFs or promote immutable producer flags. Frames
are compared whole and unchanged against their own sealed acquisition manifest.
"""
import ast
import gc
import importlib.util
import json
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
HERE = Path(__file__).parent
spec = importlib.util.spec_from_file_location('wrap_historical', HERE/'vis36-wrap-check.py')
audit = importlib.util.module_from_spec(spec); spec.loader.exec_module(audit)
check, TMP, OUT = audit.check, audit.TMP, audit.OUT

def handoff():
    before = check.load(audit.BEFORE); audit.preservation(before)
    acquired = check.load(TMP/'vis36-wrap-acquisition.json')
    current = audit.current_source()
    assert current != acquired['source'], 'historical-only guard: current qualification belongs to another observer'
    original = TMP/'t44-reference/package/bin/opencode'
    assert check.elf(original) == acquired['ELF'][str(original)]
    inverses = {}
    for role in ('wide','small','gates'):
        name = 'vis36-wrap-'+role; inv = check.load(OUT/(name+'.inventory.json'))
        proof = audit.inverse(OUT/(name+'.tar.xz'), inv)
        assert all(inv[k] == proof[k] for k in ('archive_sha256','archive_bytes','tar_sha256','tar_bytes','xz'))
        assert inv['inventory'] == proof['tar']['inventory']
        inverses[role] = proof
    # Only reuse the immutable whole-frame comparator declaration. Source identity
    # here is explicitly the captured epoch; the result below records current drift.
    tree = audit.declarations(audit.R0, {'whole_frames'})
    replaced = 0
    for n in ast.walk(tree):
        if isinstance(n, ast.Constant) and n.value == '007': n.value = '008'
        if isinstance(n, ast.Assign) and any(isinstance(t, ast.Name) and t.id == 'source' for t in n.targets):
            assert ast.unparse(n.value) == 'current_source()'
            n.value = ast.parse("acquired['source']",mode='eval').body; replaced += 1
    assert replaced == 1
    ns = {**audit.__dict__, 'acquired':acquired}
    exec(compile(ast.fix_missing_locations(tree), str(audit.R0), 'exec'), ns)
    frames = {}
    for p in audit.PROFILES:
        root = audit.roots(p)['current']; lock = check.load(root/'capture.lock.json')
        for side,path in (('oc',audit.REPO/'target/debug/oc'), ('upstream',original)):
            assert lock[side]['executable_sha256'] == acquired['ELF'][str(path)]['sha256']
        frames[p] = ns['whole_frames'](p)
        grids = [check.load(root/s/'permission-mcp-completed.cells.json') for s in ('upstream','oc')]
        wanted = (26,27) if p == 'wide' else (25,26,27)
        assert all(grids[0]['cells'][y] == grids[1]['cells'][y] for y in wanted)
        frames[p]['complete_summary_equal_rows'] = list(wanted)
        assert {s:audit.effects_ns['effects'](root,s) for s in ('oc','upstream')} == acquired['profiles'][p]['effects']
    records = {}
    for path in sorted(TMP.glob('vis37-form-gate-vis36-wrap-*.json')):
        row = check.load(path); data = check.read_bytes(Path(row['log']))
        summaries = re.findall(rb'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',data)
        assert len(data) == row['log_bytes'] and len(summaries) == row['rust_targets']
        assert [sum(int(r[n]) for r in summaries) for n in range(3)] == [row[k] for k in ('passed','failed','ignored')]
        records[row['label']] = {**row, 'log_sha256':check.sha(data), 'record_sha256':check.digest(path),
                                'current_source_qualified':False, 'source_epoch':'UNSEALED_DURING_CONCURRENT_DRIFT'}
    state = check.load(audit.REPO/'progress/STATE.json')
    assert state['current'] == 'T44' and state['tasks']['T44']['status'] == 'active' and state['tasks']['T44']['last_seq'] == 145
    assert {k:v for k,v in state['tasks'].items() if k != 'T44'} == {k:v for k,v in before['state']['tasks'].items() if k != 'T44'}
    result = dict(status='HISTORICAL_CAPTURE_CHECKED_CURRENT_QUALIFICATION_INTERRUPTED_NOT_DELIVERED',
                  captured_source_sha256=acquired['source_sha256'], captured_source_count=acquired['source_count'],
                  current_source_sha256=check.producer_sha(current), current_source_count=len(current),
                  added_source_paths=sorted(set(current)-set(acquired['source'])),
                  changed_source_paths=sorted(k for k in current if k in acquired['source'] and current[k] != acquired['source'][k]),
                  acquisition_ELF=acquired['ELF'], observed_current_ELF={str(p):check.elf(p) for p in (audit.REPO/'target/debug/oc',audit.REPO/'target/release/oc',original)},
                  profiles=frames, effects={p:v['effects'] for p,v in acquired['profiles'].items()}, inverses=inverses,
                  observed_gates=records, interruption=check.load(TMP/'vis36-wrap-workspace-interrupted.json'),
                  prior_proofs=len(before['prior']), original_files=len(before['original_files']), original_directories=len(before['original_dirs']),
                  quota=check.quota(), memory=check.proc_memory(), state=state)
    print(json.dumps({k:result[k] for k in ('status','captured_source_sha256','current_source_count','changed_source_paths','quota')}))
    return result

if __name__ == '__main__':
    assert len(sys.argv) == 2
    audit.save(Path(sys.argv[1]), handoff())
