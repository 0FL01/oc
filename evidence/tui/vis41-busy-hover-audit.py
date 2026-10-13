"""Independent original-byte Prompt inverse; visual hover is not close authority."""
import ast
import collections
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import resource
import subprocess
import sys

sys.dont_write_bytecode = True
resource.setrlimit(resource.RLIMIT_AS, (384 * 1024 * 1024,) * 2)
REPO = Path('/home/opencode/ai/oc')
TMP = Path('/home/opencode/.cache/opencode-tmp/opencode')
OUT = REPO / 'evidence/tui'
BASE = 'e6ef87ae16e2ba1a6c5f7f81159a1eda9f8ed5bc'
assert os.getuid() == 1003 and subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip() == BASE

def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

owner = load(TMP / 'vis37-form-independent-20261012-sole-verifier/owner_refinement.py', 'busy_hover_owner')
v, check = owner.v, owner.v.inverse
check.PREFIXES = tuple(dict.fromkeys((*check.PREFIXES, 'vis36-')))
ROOTS = {'current-wide': TMP / 'vis36-prompt-current-002/prompt-wide-r2',
         'current-small': TMP / 'vis36-prompt-current-002/prompt-small-r2',
         'current-logs': TMP / 'vis36-prompt-current-002', 'current-driver': TMP / 'vis36-prompt-capture-r2.py'}
v.ROOTS = check.ROOTS = ROOTS
check.HEAD = v.frames.HEAD = BASE
parsed = ast.parse((OUT / 'vis36-prompt-audit.py').read_text())
stages = [n for n in parsed.body if isinstance(n, ast.Assign) and any(isinstance(t, ast.Name) and t.id == 'STAGES' for t in n.targets)]
assert len(stages) == 1
exec(compile(ast.Module(body=stages, type_ignores=[]), __file__, 'exec'), globals())
functions = []
for filename, names in [('vis37-cancel-intermediate-audit.py', {'save'}),
                        ('vis37-focus-paint-audit.py', {'strict_commands'}),
                        ('vis36-prompt-audit.py', {'effects', 'original_inventory'})]:
    selected = [n for n in ast.parse((OUT / filename).read_text()).body if isinstance(n, ast.FunctionDef) and n.name in names]
    assert {n.name for n in selected} == names
    if 'strict_commands' in names:
        counts = [n for n in ast.walk(selected[0]) if isinstance(n, ast.Constant) and n.value == 36]
        assert len(counts) == 1
        counts[0].value = 2 * len(STAGES)
    functions += selected
exec(compile(ast.fix_missing_locations(ast.Module(body=functions, type_ignores=[])), __file__, 'exec'), globals())
BEFORE, AFTER = OUT / 'vis41-busy-hover-before.json', OUT / 'vis41-busy-hover-after.json'

def source_and_elf():
    source = v.source()
    previous = json.loads((TMP / 'vis36-prompt-current-001/prompt-wide-r1/source-manifest.json').read_bytes())
    assert len(source) == 713 and set(source) == set(previous)
    assert {p for p in source if source[p] != previous[p]} == {
        'crates/oc-tui/src/app/tabs.rs', 'crates/oc-tui/src/app/tests/tabs.rs',
        'crates/oc-tui/src/shell.rs', 'crates/oc-tui/src/shell/tests.rs'}
    old = json.loads((TMP / 'vis36-prompt-current-001/prompt-wide-r1/capture.lock.json').read_bytes())
    result = {}
    for role in ('current-wide', 'current-small'):
        root = ROOTS[role]
        lock = json.loads((root / 'capture.lock.json').read_bytes())
        assert lock['oc']['commit'] == BASE and source == json.loads((root / 'source-manifest.json').read_bytes())
        assert check.producer_sha(source) == lock['oc']['source_manifest_sha256']
        assert lock['sources']['upstream_commit'] == '2670273ff17da96f85c5826ced57aa1b368754fa'
        commands = json.loads((root / 'commands.json').read_bytes())
        expected = {'--reference': str(TMP / 't44-reference/package/bin/opencode'), '--oc': str(REPO / 'target/debug/oc'),
                    '--build-oc': 'true', '--geometry': 'true', '--sample': 'short', '--sidebar': 'hide', '--permission': 'true',
                    '--permission-mode': 'prompt', '--columns': str(121 if role.endswith('wide') else 79), '--rows': '40', '--output': str(root)}
        argv = commands[0]['argv']
        pairs = list(zip(argv[2::2], argv[3::2], strict=True))
        assert commands[0]['role'] == 'capture runner invocation' and argv[1] == str(REPO / 'scripts/tui_capture/capture.mjs')
        assert len(pairs) == len(expected) and dict(pairs) == expected
        builds = [i for i in commands if i['argv'][0] == 'cargo']
        assert len(builds) == 1 and builds[0]['argv'] == ['cargo', 'build', '--locked'] and builds[0]['exit_code'] == 0
        for side in ('oc', 'upstream'):
            elf = v.frames.elf(Path(lock[side]['executable_path']))
            assert elf['sha256'] == lock[side]['executable_sha256']
            assert (elf['sha256'] != old[side]['executable_sha256']) == (side == 'oc')
            if side in result:
                assert result[side] == elf
            result[side] = elf
        strict_commands(root, lock, commands)
    return {'source': source, 'elf': result}

def hover_visual(root):
    records = []
    for stage in ('once-returned-ask', 'once-fullscreen', 'once-inline'):
        grids = [check.load(root / side / ('permission-' + stage + '.cells.json')) for side in ('upstream', 'oc')]
        cells = [g['cells'][0] for g in grids]
        positions = [[x for x, c in enumerate(row) if c['symbol'] == '✕'] for row in cells]
        assert positions[0] == positions[1] and len(positions[0]) == 1
        x = positions[0][0]
        assert cells[0][x] == cells[1][x]
        records.append(dict(stage=stage, x=x, y=0, styled_cell=cells[0][x],
                            scope='paint only; busy close refusal is established separately by typed native tests'))
    return records

def before():
    helpers = [Path(__file__), TMP / 'vis37-form-gate-20261012.py',
               TMP / 'vis37-form-independent-20261012-sole-verifier/owner_refinement.py',
               TMP / 'vis37-form-independent-20261012-sole-verifier/verifier.py',
               OUT / 'vis36-prompt-audit.py', OUT / 'vis37-cancel-intermediate-audit.py',
               OUT / 'vis37-focus-paint-audit.py', *sorted(v.OLD_HELPERS.glob('*.py'))]
    older = [p for p in OUT.iterdir() if p.name.endswith(('.tar.xz', '.inventory.json', '.archived.json'))]
    record = lambda p: dict(sha256=check.digest(p), metadata=check.metadata(p))
    result = dict(status='INDEPENDENT_BUSY_HOVER_BEFORE_SEALED', original_inventory=original_inventory(),
                  source_and_elf=source_and_elf(), helpers={str(p): record(p) for p in helpers},
                  prior_proofs={str(p): record(p) for p in older},
                  effects={role: {s: effects(ROOTS[role], s) for s in ('oc', 'upstream')} for role in ('current-wide', 'current-small')},
                  hover_visual={role: hover_visual(ROOTS[role]) for role in ('current-wide', 'current-small')})
    save(BEFORE, result)

def after():
    before = json.loads(BEFORE.read_bytes())
    assert original_inventory() == before['original_inventory'] and source_and_elf() == before['source_and_elf']
    pairs, inverses = [], {}
    for profile in ('wide', 'small'):
        role = 'current-' + profile
        path = OUT / ('vis36-busy-hover-' + profile + '-proof.tar.xz')
        receipt = json.loads(path.with_name(path.name.removesuffix('.tar.xz') + '.inventory.json').read_bytes())
        packed = path.read_bytes()
        assert check.sha(packed) == receipt['archive_sha256']
        raw, xz = check.inverse_xz(packed)
        check.ROOTS = {'current': ROOTS[role], 'current-logs': ROOTS['current-logs'], 'current-driver': ROOTS['current-driver']}
        try:
            restored, tar = check.inverse_ustar(raw, before['original_inventory'][profile]['files'])
        finally:
            check.ROOTS = ROOTS
        assert tar['inventory'] == receipt['inventory'] and check.sha(raw) == receipt['tar_sha256']
        inverses[profile] = {'xz': xz, 'tar': tar}
        del raw, restored, packed
        root = ROOTS[role]
        lock = json.loads((root / 'capture.lock.json').read_bytes())
        assert [i['scenario'] for i in lock['attempts'] if i['mode'] == 'grid'] == ['permission-' + s for s in STAGES]
        for stage in STAGES:
            base = 'permission-' + stage
            grid, _ = v.frames.compare_grid(*(check.load(root / side / (base + '.cells.json')) for side in ('upstream', 'oc')))
            png = v.frames.compare_png(root / 'upstream' / (base + '.png'), root / 'oc' / (base + '.png'))
            recorded_grid = check.load(root / (base + '.grid-diff.json'))
            recorded_png = check.load(root / (base + '.png-diff.json'))
            assert all(grid[k] == recorded_grid[k] for k in grid if k != 'different_fields') and all(png[k] == recorded_png[k] for k in png)
            assert recorded_grid['status'] == ('PASS' if grid['different_cells'] == 0 and not grid['cursor_differs'] else 'FAIL')
            assert recorded_png['status'] == ('PASS' if png['different_pixels'] == 0 else 'FAIL')
            pairs.append(dict(profile=profile, stage=stage, grid=grid, png=png))
    assert original_inventory() == before['original_inventory'] and source_and_elf() == before['source_and_elf']
    assert all(dict(sha256=check.digest(p), metadata=check.metadata(p)) == record for group in ('helpers', 'prior_proofs') for p, record in before[group].items())
    effects_now = {role: {s: effects(ROOTS[role], s) for s in ('oc', 'upstream')} for role in ('current-wide', 'current-small')}
    visuals = {role: hover_visual(ROOTS[role]) for role in ('current-wide', 'current-small')}
    assert effects_now == before['effects'] and visuals == before['hover_visual']
    totals = dict(pairs=len(pairs), pngs=2 * len(pairs), styled_cells=sum(p['grid']['cells_checked'] for p in pairs),
                  differing_cells=sum(p['grid']['different_cells'] for p in pairs), pixels=sum(p['png']['pixels_checked'] for p in pairs),
                  differing_pixels=sum(p['png']['different_pixels'] for p in pairs), cursor_pairs_equal=sum(not p['grid']['cursor_differs'] for p in pairs),
                  whole_grids_equal=sum(p['grid']['different_cells'] == 0 and not p['grid']['cursor_differs'] for p in pairs),
                  whole_pngs_equal=sum(p['png']['different_pixels'] == 0 for p in pairs))
    save(AFTER, dict(status='INDEPENDENT_BUSY_HOVER_FULL_BYTE_EXACT_RECOUNT_T44_ACTIVE_NOT_PASS', inverses=inverses,
                     totals=totals, pairs=pairs, effects=effects_now, hover_visual=visuals,
                     preserved_prior_proofs=len(before['prior_proofs']), source_and_elf=before['source_and_elf'], memory=check.proc_memory(), quota=check.quota()))

if __name__ == '__main__':
    {'before': before, 'after': after}[sys.argv[1]]()
