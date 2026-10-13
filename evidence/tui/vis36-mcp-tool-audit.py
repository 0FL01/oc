"""Independent Prompt004 bytes/metadata, manual XZ/USTAR inverse and strict frames."""
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
resource.setrlimit(resource.RLIMIT_AS, (384 * 1024**2,) * 2)
REPO = Path('/home/opencode/ai/oc')
TMP = Path('/home/opencode/.cache/opencode-tmp/opencode')
OUT = REPO / 'evidence/tui'
BASE = 'daf5e6bf58210cac686e5b113ddc149fbad248f7'
assert os.getuid() == 1003

def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

owner = load(TMP / 'vis37-form-independent-20261012-sole-verifier/owner_refinement.py', 'mcp_tool_owner')
v, check = owner.v, owner.v.inverse
check.PREFIXES = tuple(dict.fromkeys((*check.PREFIXES, 'vis36-')))
ROOTS = {'current-wide': TMP / 'vis36-prompt-current-004/prompt-wide-r4',
         'current-small': TMP / 'vis36-prompt-current-004/prompt-small-r4',
         'current-logs': TMP / 'vis36-prompt-current-004',
         'current-driver': TMP / 'vis36-prompt-capture-r4.py'}
v.ROOTS = check.ROOTS = ROOTS
check.HEAD = v.frames.HEAD = BASE
# Load only selected immutable declarations, never old entrypoints/HEAD guards.
nodes = []
for filename, names in [('vis37-cancel-intermediate-audit.py', {'save'}),
                        ('vis37-focus-paint-audit.py', {'strict_commands'}),
                        ('vis36-prompt-audit.py', {'effects', 'original_inventory'}),
                        ('vis41-busy-hover-audit.py', {'hover_visual'})]:
    parsed = ast.parse((OUT / filename).read_text())
    selected = [n for n in parsed.body if isinstance(n, ast.FunctionDef) and n.name in names]
    assert {n.name for n in selected} == names
    if filename == 'vis36-prompt-audit.py':
        stages = [n for n in parsed.body if isinstance(n, ast.Assign) and
                  any(isinstance(t, ast.Name) and t.id == 'STAGES' for t in n.targets)]
        assert len(stages) == 1
        nodes += stages
    if 'strict_commands' in names:
        counts = [n for n in ast.walk(selected[0]) if isinstance(n, ast.Constant) and n.value == 36]
        assert len(counts) == 1
        counts[0].value = 102
    nodes += selected
exec(compile(ast.fix_missing_locations(ast.Module(body=nodes, type_ignores=[])), __file__, 'exec'), globals())
BEFORE, AFTER = OUT / 'vis36-mcp-tool-before.json', OUT / 'vis36-mcp-tool-after.json'

def source_and_elf():
    source = v.source()
    previous = check.load(TMP / 'vis36-prompt-current-003/prompt-wide-r3/source-manifest.json')
    added = {'crates/oc-core/src/approval_tool_tests.rs', 'crates/oc-tui/src/approval_view_tool_tests.rs'}
    changed = {'crates/oc-core/src/approval.rs', 'crates/oc-adapters/src/approval.rs',
               'crates/oc-adapters/src/approval_tests.rs', 'crates/oc-tui/src/approval_view.rs'}
    assert len(source) == 715 and set(source) - set(previous) == added and not set(previous) - set(source)
    assert {p for p in previous if source[p] != previous[p]} == changed
    old = check.load(TMP / 'vis36-prompt-current-003/prompt-wide-r3/capture.lock.json')
    result = {}
    for role in ('current-wide', 'current-small'):
        root = ROOTS[role]
        lock = check.load(root / 'capture.lock.json')
        assert lock['oc']['commit'] == BASE and source == check.load(root / 'source-manifest.json')
        assert check.producer_sha(source) == lock['oc']['source_manifest_sha256']
        assert lock['sources']['upstream_commit'] == '2670273ff17da96f85c5826ced57aa1b368754fa'
        commands = check.load(root / 'commands.json')
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

def tool_body(root):
    records = []
    for side, action in [('upstream', 'vis36_echo'), ('oc', 'vis36__echo')]:
        protocol = check.load(root / side / 'protocol.json')
        calls = [i for i in protocol if i['kind'] == 'fixture_tool_call' and i['case'] == 'mcp']
        assert len(calls) == 1
        # The typed producer test proves action != policy resource '*'. These
        # real frames retain each origin's intentional tool spelling verbatim.
        for stage in ('mcp-fullscreen', 'mcp-inline'):
            grid = check.load(root / side / ('permission-' + stage + '.cells.json'))
            hits = []
            for y, row in enumerate(grid['cells']):
                text = ''.join(c['symbol'] for c in row)
                if 'Tool: ' + action in text:
                    x = text.index('Tool: ' + action)
                    hits.append({'x': x, 'y': y, 'text': 'Tool: ' + action,
                                 'cells': row[x:x + len('Tool: ' + action)]})
                assert 'Resource: *' not in text and 'Tool: *' not in text
            assert len(hits) == 1
            records.append({'origin': side, 'stage': stage, 'actual_action': action,
                            'body': hits[0], 'scope': 'semantic action diagnostic only; whole frames remain strict'})
    return records

def legacy_seals():
    # Literal old episode parents; full bytes and all directory metadata. This
    # never runs a capture, imports an old entrypoint or reads authoring config.
    roots = [TMP / ('vis36-prompt-current-' + n) for n in ('001', '002', '003')]
    roots += [TMP / ('vis36-mode-current-' + f'{n:03}') for n in range(1, 11)]
    roots += [TMP / ('vis37-current-' + n) for n in ('001', '002')]
    roots += [TMP / ('vis37-form-current-' + f'{n:03}') for n in range(1, 13)]
    files, directories = {}, {}
    for root in roots:
        assert root.is_dir()
        for p in [root, *sorted(root.rglob('*'))]:
            meta = check.metadata(p)
            if p.is_dir():
                directories[str(p)] = meta
            else:
                assert p.is_file() and meta[0] <= check.FILE
                files[str(p)] = [check.digest(p), meta]
                assert check.metadata(p) == meta
    return {'files': files, 'directories': directories}

def before():
    assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip() == BASE
    helpers = [Path(__file__), TMP / 'vis37-form-gate-20261012.py',
               TMP / 'vis37-form-independent-20261012-sole-verifier/owner_refinement.py',
               TMP / 'vis37-form-independent-20261012-sole-verifier/verifier.py',
               OUT / 'vis36-prompt-audit.py', OUT / 'vis37-cancel-intermediate-audit.py',
               OUT / 'vis37-focus-paint-audit.py', OUT / 'vis41-busy-hover-audit.py',
               OUT / 'vis41-busy-hover-final-audit.py', TMP / 'vis41-busy-hover-delivery-check.py',
               TMP / 'vis41-busy-hover-delivery-gates.py', *sorted(v.OLD_HELPERS.glob('*.py')),
               *sorted(TMP.glob('vis36-*.py')), *sorted(TMP.glob('vis37-*.py'))]
    helpers = sorted(set(helpers))
    older = [p for p in OUT.iterdir() if p.name.endswith(('.tar.xz', '.inventory.json', '.archived.json'))]
    record = lambda p: dict(sha256=check.digest(p), metadata=check.metadata(p))
    result = dict(status='INDEPENDENT_MCP_TOOL_BEFORE_SEALED', original_inventory=original_inventory(),
                  source_and_elf=source_and_elf(), helpers={str(p): record(p) for p in helpers},
                  prior_proofs={str(p): record(p) for p in older}, legacy=legacy_seals(),
                  effects={role: {s: effects(ROOTS[role], s) for s in ('oc', 'upstream')} for role in ('current-wide', 'current-small')},
                  hover_visual={role: hover_visual(ROOTS[role]) for role in ('current-wide', 'current-small')},
                  tool_body={role: tool_body(ROOTS[role]) for role in ('current-wide', 'current-small')})
    save(BEFORE, result)

def seals(before):
    assert original_inventory() == before['original_inventory'] and source_and_elf() == before['source_and_elf']
    assert legacy_seals() == before['legacy']
    assert all(dict(sha256=check.digest(p), metadata=check.metadata(p)) == record
               for group in ('helpers', 'prior_proofs') for p, record in before[group].items())
    for name, function in [('effects', lambda root: {s: effects(root, s) for s in ('oc', 'upstream')}),
                           ('hover_visual', hover_visual), ('tool_body', tool_body)]:
        assert {role: function(ROOTS[role]) for role in ('current-wide', 'current-small')} == before[name]

def after():
    before = check.load(BEFORE)
    seals(before)
    pairs, inverses = [], {}
    for profile in ('wide', 'small'):
        role = 'current-' + profile
        path = OUT / ('vis36-mcp-tool-' + profile + '-proof.tar.xz')
        receipt = check.load(path.with_name(path.name.removesuffix('.tar.xz') + '.inventory.json'))
        packed = check.read_bytes(path)
        assert check.sha(packed) == receipt['archive_sha256'] and len(packed) == receipt['archive_bytes']
        raw, xz = check.inverse_xz(packed)
        check.ROOTS = {'current': ROOTS[role], 'current-logs': ROOTS['current-logs'], 'current-driver': ROOTS['current-driver']}
        try:
            restored, tar = check.inverse_ustar(raw, before['original_inventory'][profile]['files'])
        finally:
            check.ROOTS = ROOTS
        assert tar['inventory'] == receipt['inventory'] and check.sha(raw) == receipt['tar_sha256'] and len(raw) == receipt['tar_bytes']
        inverses[profile] = {'xz': xz, 'tar': tar, 'archive_sha256': check.sha(packed)}
        del raw, restored, packed
        root = ROOTS[role]
        lock = check.load(root / 'capture.lock.json')
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
    seals(before)
    totals = dict(pairs=len(pairs), pngs=2 * len(pairs), styled_cells=sum(p['grid']['cells_checked'] for p in pairs),
                  differing_cells=sum(p['grid']['different_cells'] for p in pairs), pixels=sum(p['png']['pixels_checked'] for p in pairs),
                  differing_pixels=sum(p['png']['different_pixels'] for p in pairs), cursor_pairs_equal=sum(not p['grid']['cursor_differs'] for p in pairs),
                  whole_grids_equal=sum(p['grid']['different_cells'] == 0 and not p['grid']['cursor_differs'] for p in pairs),
                  whole_pngs_equal=sum(p['png']['different_pixels'] == 0 for p in pairs))
    save(AFTER, dict(status='INDEPENDENT_MCP_TOOL_FULL_BYTE_EXACT_RECOUNT_T44_ACTIVE_NOT_PASS', inverses=inverses,
                     totals=totals, pairs=pairs, effects=before['effects'], hover_visual=before['hover_visual'], tool_body=before['tool_body'],
                     before_sha256=check.digest(BEFORE), preserved_prior_proofs=len(before['prior_proofs']),
                     legacy_files=len(before['legacy']['files']), legacy_directories=len(before['legacy']['directories']),
                     source_and_elf=before['source_and_elf'], memory=check.proc_memory(), quota=check.quota()))

if __name__ == '__main__':
    {'before': before, 'after': after}[sys.argv[1]]()
