"""Independent source-space slice audit: pure reused codec declarations, fresh seals."""
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

sys.dont_write_bytecode = True
resource.setrlimit(resource.RLIMIT_AS, (384 * 1024**2,) * 2)
REPO = Path('/home/opencode/ai/oc')
TMP = Path('/home/opencode/.cache/opencode-tmp/opencode')
OUT = REPO / 'evidence/tui'
BASE = 'bf0cfecd70cef9ae21646ed96a6c8c7e50581edb'
BEFORE = TMP / 'vis36-wrap-before.json'
R0 = OUT / 'vis36-generic-summary-check.py'
tree = ast.parse(R0.read_bytes())
node = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'declarations')
exec(compile(ast.fix_missing_locations(ast.Module(body=[node], type_ignores=[])), str(R0), 'exec'))
check = types.ModuleType('independent_wrap_inverse')
old = TMP / 'vis37-independent-20261012-sole-verifier/inverse_verifier.py'
names = {'require','sha','digest','read_bytes','load','canonical_sha','metadata','git','producer_sha','proc_memory','quota',
         'vli','inverse_xz','cstring','octal','make_header','path_ok','inverse_ustar','compare_grid','decode_png','compare_png','elf'}
exec(compile(declarations(old, names, ('SCOPE','PREFIXES'), True), str(old), 'exec'), check.__dict__)
check.AS, check.FILE, check.TOTAL, check.OWNED = 384*1024**2, 16*1024**2, 128*1024**2, 1024**3
check.MEMBERS, check.XZ_DICT, check.XZ_DECODE = 2000, 16*1024**2, 32*1024**2
check.REPO, check.TEMP = REPO, TMP
check.PREFIXES = (*check.PREFIXES, 'vis36-')
check.ROOTS = {}
check.DONOR = '2670273ff17da96f85c5826ced57aa1b368754fa'
check.HEAD = BASE
check.FIELDS = {'symbol','fg','bg','modifiers','width'}
exec(compile(declarations(R0, {'module','current_source','record','save','preservation','inventory','inverse','whole_frames'}), str(R0), 'exec'))
sys.path.insert(0, str(REPO / 'scripts'))
producer = module('wrap_strict_encoder', REPO / 'scripts/retain_syntax_proof.py')
effects_ns = dict(json=json, collections=check.collections)
exec(compile(declarations(OUT / 'vis36-prompt-audit.py', {'effects'}, ('STAGES',)), 'effects', 'exec'), effects_ns)
STAGES = effects_ns['STAGES']
PROFILES = ('wide', 'small')

def roots(profile):
    return producer.vis36_prompt_roots(profile, '008')

def baseline():
    assert os.getuid() == 1003 and check.git('rev-parse','HEAD').decode().strip() == BASE
    assert not check.git('diff','--name-only') and not check.git('diff','--cached','--name-only')
    before = check.load(TMP / 'vis36-generic-summary-before.json')
    preservation(before)
    files, dirs = dict(before['original_files']), dict(before['original_dirs'])
    for path in sorted(OUT.glob('*.inventory.json')):
        inv = check.load(path)
        for row in inv.get('original_members', {}).values():
            if Path(row['path']).is_file():
                files[row['path']] = [row['sha256'], row['original_metadata']]
        dirs.update(inv.get('original_directories', {}))
    helpers = dict(before['helpers'])
    for path in [*sorted(TMP.glob('vis36-*.py')), *sorted(TMP.glob('vis37-*.py')), *sorted(OUT.glob('vis*.py'))]:
        if path != Path(__file__): helpers[str(path)] = record(path)
    prior = {str(p):record(p) for p in sorted(OUT.iterdir()) if p.name.endswith(('.tar.xz','.inventory.json','.archived.json'))}
    source = current_source()
    assert len(source) == 715 and check.producer_sha(source) == 'e8dc500cd1d1b74b8499a62152a58b9846d764b7eb361c0828b8813be554675e'
    value = dict(source=source, prior=prior, helpers=helpers, original_files=files, original_dirs=dirs,
                 state=check.load(REPO / 'progress/STATE.json'), quota=check.quota())
    preservation(value); save(BEFORE, value)
    print(json.dumps(dict(status='BASELINE_SEALED', prior=len(prior), files=len(files), directories=len(dirs), quota=value['quota'])))

def acquisition():
    before = check.load(BEFORE); preservation(before)
    source = current_source()
    changed = sorted(k for k in source if source[k] != before['source'].get(k))
    assert set(source) == set(before['source']) and changed == ['crates/oc-tui/src/tools.rs', 'crates/oc-tui/src/tools/presentation_tests.rs']
    value = dict(source=source, source_sha256=check.producer_sha(source), source_count=len(source), changed_inputs=changed,
                 ELF={str(p):check.elf(p) for p in (REPO/'target/debug/oc', REPO/'target/release/oc', TMP/'t44-reference/package/bin/opencode')},
                 support_inputs={str(REPO/p):check.digest(REPO/p) for p in ('scripts/retain_syntax_proof.py','scripts/test_retain_syntax_proof.py')}, profiles={})
    for profile in PROFILES:
        mapping = roots(profile); producer.ROOTS = mapping
        files = producer.source_files()
        value['profiles'][profile] = {**inventory(mapping, files), 'strict_producer_checks':producer.vis36_prompt_checks(files,profile,'008'),
                                     'effects':{s:effects_ns['effects'](mapping['current'],s) for s in ('oc','upstream')}}
        del files; gc.collect()
    value['quota'] = check.quota(); save(TMP/'vis36-wrap-acquisition.json', value)
    print(json.dumps(dict(status='ACQUISITION_SEALED', source=value['source_sha256'], profiles={p:dict(members=len(v['original_members']), logical_bytes=sum(r['bytes'] for r in v['original_members'].values())) for p,v in value['profiles'].items()})))

def frames(profile):
    # Reuse only the comparator declaration; its literal old attempt is replaced
    # with this fixed source-specific episode, without running an old entrypoint.
    tree = declarations(R0, {'whole_frames'})
    for n in ast.walk(tree):
        if isinstance(n, ast.Constant) and n.value == '007': n.value = '008'
    ns = dict(globals()); exec(compile(ast.fix_missing_locations(tree), str(R0), 'exec'), ns)
    result = ns['whole_frames'](profile)
    root = roots(profile)['current']
    grids = [check.load(root/s/'permission-mcp-completed.cells.json') for s in ('upstream','oc')]
    wanted = (26,27) if profile == 'wide' else (25,26,27)
    assert all(grids[0]['cells'][y] == grids[1]['cells'][y] for y in wanted)
    result['complete_summary_equal_rows'] = list(wanted)
    return result

def gates():
    records = {}
    for path in sorted(TMP.glob('vis37-form-gate-vis36-wrap-*.json')):
        row = check.load(path); data = check.read_bytes(Path(row['log']))
        summaries = re.findall(rb'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', data)
        assert len(data) == row['log_bytes'] and len(summaries) == row['rust_targets']
        assert [sum(int(r[n]) for r in summaries) for n in range(3)] == [row[k] for k in ('passed','failed','ignored')]
        records[row['label']] = {**row, 'log_sha256':check.sha(data), 'record_sha256':check.digest(path)}
    for label in ('green-r1','clippy','fmt','crate','workspace-r1','debug-locked','debug-ordinary','debug-help','release-build','release-help','python','ordinary-debug-help','ordinary-release-help'):
        assert records['vis36-wrap-'+label]['exit_code'] == 0, label
    assert records['vis36-wrap-red']['exit_code'] != 0 and records['vis36-wrap-red']['failed'] == 1
    assert records['vis36-wrap-green']['exit_code'] != 0 and records['vis36-wrap-green']['failed'] == 1
    assert records['vis36-wrap-green-r1']['passed'] == 12 and records['vis36-wrap-crate']['passed'] == 537
    w = records['vis36-wrap-workspace-r1']
    assert w['exit_code'] == 0 and w['passed'] == 1875 and w['failed'] == 0 and w['ignored'] == 11 and w['rust_targets'] == 46
    assert '--locked' in w['command'] and '--no-fail-fast' in w['command']
    for label,count in {'startup':2,'term01':1,'panic':1,'vis31':2,'s07':1,'vis38':1,'aud32':1}.items():
        r = records['vis36-wrap-release-'+label]
        assert r['exit_code'] == 0 and r['passed'] == count and r['failed'] == 0 and r['rust_targets'] == 1
        if label in ('s07','vis38'): assert '--exact' not in r['command']
    assert re.search(rb'Ran 71 tests in .*\n\nOK\n', check.read_bytes(TMP/'vis37-form-gate-vis36-wrap-python.log'))
    return dict(attempts=records, workspace_passed=1875, affected_crate_passed=537, release_cases=9, python_tests=71,
                interruption=check.load(TMP/'vis36-wrap-workspace-interrupted.json'))

def audit(delivery=False):
    before = check.load(BEFORE); preservation(before)
    acquired = check.load(TMP/'vis36-wrap-acquisition.json')
    assert current_source() == acquired['source']
    assert {p:check.digest(Path(p)) for p in acquired['support_inputs']} == acquired['support_inputs']
    ordinary = check.load(TMP/'vis36-wrap-ordinary-ELF.json')
    assert {p:check.elf(Path(p)) for p in ordinary} == ordinary
    assert ordinary[str(TMP/'t44-reference/package/bin/opencode')]['sha256'] == '2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a'
    inverses = {}
    for role in ('wide','small','gates'):
        name = 'vis36-wrap-'+role; inv = check.load(OUT/(name+'.inventory.json'))
        actual = inverse(OUT/(name+'.tar.xz'), inv)
        assert all(inv[k] == actual[k] for k in ('archive_sha256','archive_bytes','tar_sha256','tar_bytes','xz')) and inv['inventory'] == actual['tar']['inventory']
        inverses[role] = actual
    comparisons = {p:frames(p) for p in PROFILES}
    for profile in PROFILES:
        producer.ROOTS = roots(profile); files = producer.source_files()
        assert producer.vis36_prompt_checks(files,profile,'008') == acquired['profiles'][profile]['strict_producer_checks']
        assert {s:effects_ns['effects'](roots(profile)['current'],s) for s in ('oc','upstream')} == acquired['profiles'][profile]['effects']
        del files; gc.collect()
    state = check.load(REPO/'progress/STATE.json')
    if delivery:
        assert state['current'] == 'T44' and state['tasks']['T44']['status'] == 'active' and state['tasks']['T44']['last_seq'] == 145
        assert state['schema_version'] == before['state']['schema_version']
        assert {k:v for k,v in state['tasks'].items() if k != 'T44'} == {k:v for k,v in before['state']['tasks'].items() if k != 'T44'}
        assert check.git('branch','--show-current').decode().strip() == 'agent/oc-rust-port'
        assert check.git('remote','get-url','origin').decode().strip() == 'git@github.com:0FL01/oc.git'
    result = dict(status='CHECKED_SOURCE_SPACE_SLICE_NOT_T44_PASS', source_sha256=acquired['source_sha256'], source_count=acquired['source_count'],
                  acquisition_ELF=acquired['ELF'], ordinary_ELF=ordinary, profiles=comparisons, effects={p:v['effects'] for p,v in acquired['profiles'].items()},
                  inverses=inverses, gates=gates(), prior_proofs=len(before['prior']), original_files=len(before['original_files']), original_directories=len(before['original_dirs']),
                  quota=check.quota(), memory=check.proc_memory(), state=state)
    print(json.dumps(dict(status=result['status'], source=result['source_sha256'], profiles={p:{k:v for k,v in f.items() if k not in ('frames','summary_rows')} for p,f in comparisons.items()}, quota=result['quota'])))
    return result

if __name__ == '__main__':
    if sys.argv[1:] == ['baseline']: baseline()
    elif sys.argv[1:] == ['acquisition']: acquisition()
    elif len(sys.argv) == 3 and sys.argv[1] in ('audit','delivery'): save(Path(sys.argv[2]), audit(sys.argv[1] == 'delivery'))
    else: raise SystemExit('baseline | acquisition | audit/delivery EXCLUSIVE_REPORT')
