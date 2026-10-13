"""Independent bounded original-byte inverse and full-frame Prompt recount, not pixel PASS."""
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
BASE = 'fc2f420aa87088fa12cb8f20a806483c735d8af5'
assert os.getuid() == 1003 and subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip() == BASE

def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

owner = load(TMP / 'vis37-form-independent-20261012-sole-verifier/owner_refinement.py', 'prompt_owner')
v, check = owner.v, owner.v.inverse
check.PREFIXES = tuple(dict.fromkeys((*check.PREFIXES, 'vis36-')))
ROOTS = {'current-wide': TMP / 'vis36-prompt-current-001/prompt-wide-r1',
         'current-small': TMP / 'vis36-prompt-current-001/prompt-small-r1',
         'current-logs': TMP / 'vis36-prompt-current-001', 'current-driver': TMP / 'vis36-prompt-capture-r1.py'}
v.ROOTS = check.ROOTS = ROOTS
check.HEAD = v.frames.HEAD = BASE
STAGES = ('home once-ask once-newhome once-returned-ask once-fullscreen once-inline once-completed reject-ask '
          'reject-completed always-ask always-selected always-completed restart-home restart-completed mixed-completed '
          'mcp-ask mcp-fullscreen mcp-inline mcp-completed childroot-ask childroot-fullscreen childroot-inline child-feedback '
          'child-feedback-draft childroot-completed read-ask read-fullscreen read-inline read-completed shell-ask shell-fullscreen '
          'shell-inline shell-completed glob-ask glob-fullscreen glob-inline glob-completed url-ask url-fullscreen url-inline '
          'url-completed settings-prompt settings-autoaccept settings-auto-first-escape autoonce-completed settings-palette-filtered '
          'settings-auto-reopen settings-restored-prompt settings-prompt-first-escape promptagain-ask promptagain-rejected').split()
namespace = globals()
functions = []
for filename, names in [('vis37-cancel-intermediate-audit.py', {'save'}), ('vis37-focus-paint-audit.py', {'strict_commands'})]:
    parsed = ast.parse((OUT / filename).read_text())
    selected = [n for n in parsed.body if isinstance(n, ast.FunctionDef) and n.name in names]
    assert {n.name for n in selected} == names
    if 'strict_commands' in names:
        counts = [n for n in ast.walk(selected[0]) if isinstance(n, ast.Constant) and n.value == 36]
        assert len(counts) == 1
        counts[0].value = 2 * len(STAGES)
    functions += selected
exec(compile(ast.fix_missing_locations(ast.Module(body=functions, type_ignores=[])), __file__, 'exec'), namespace)
BEFORE, AFTER = OUT / 'vis36-prompt-before.json', OUT / 'vis36-prompt-after.json'

def source_and_elf():
    source = v.source()
    assert len(source) == 713
    previous = json.loads((TMP / 'vis36-mode-current-010/cli-wide-r10/source-manifest.json').read_bytes())
    assert source == previous
    result = {}
    for role in ('current-wide', 'current-small'):
        root = ROOTS[role]
        lock = json.loads((root / 'capture.lock.json').read_bytes())
        assert lock['oc']['commit'] == BASE and source == json.loads((root / 'source-manifest.json').read_bytes())
        assert check.producer_sha(source) == lock['oc']['source_manifest_sha256']
        assert lock['sources']['upstream_commit'] == '2670273ff17da96f85c5826ced57aa1b368754fa'
        commands = json.loads((root / 'commands.json').read_bytes())
        columns = 121 if role.endswith('wide') else 79
        expected = {'--reference': str(TMP / 't44-reference/package/bin/opencode'), '--oc': str(REPO / 'target/debug/oc'),
                    '--build-oc': 'true', '--geometry': 'true', '--sample': 'short', '--sidebar': 'hide', '--permission': 'true',
                    '--permission-mode': 'prompt', '--columns': str(columns), '--rows': '40', '--output': str(root)}
        argv = commands[0]['argv']
        pairs = list(zip(argv[2::2], argv[3::2], strict=True))
        assert commands[0]['role'] == 'capture runner invocation' and argv[1] == str(REPO / 'scripts/tui_capture/capture.mjs')
        assert len(pairs) == len(expected) and dict(pairs) == expected
        builds = [i for i in commands if i['argv'][0] == 'cargo']
        assert len(builds) == 1 and builds[0]['argv'] == ['cargo', 'build', '--locked'] and builds[0]['exit_code'] == 0
        for side in ('oc', 'upstream'):
            elf = v.frames.elf(Path(lock[side]['executable_path']))
            assert elf['sha256'] == lock[side]['executable_sha256']
            old = json.loads((TMP / 'vis36-mode-current-010/cli-wide-r10/capture.lock.json').read_bytes())
            assert elf['sha256'] == old[side]['executable_sha256']
            if side in result:
                assert result[side] == elf
            result[side] = elf
        strict_commands(root, lock, commands)
    return {'source': source, 'elf': result}

def effects(root, side):
    checks = json.loads((root / side / 'permission-checks.json').read_bytes())
    protocol = json.loads((root / side / 'protocol.json').read_bytes())
    assert checks['status'] == 'PASS' and checks['origin'] == side and len(checks['checks']) == 112
    stage = lambda name: next(i for i in checks['checks'] if i['stage'] == name)
    providers = [i for i in protocol if i['kind'] == 'provider']
    mains = [i for i in providers if i['operation'] == 'patch']
    expected = {name: 2 for name in ('once', 'always', 'restart', 'mixed', 'mcp', 'childroot', 'childread', 'read', 'shell', 'autoonce')}
    expected.update({name: 1 for name in ('reject', 'glob', 'url', 'promptagain')})
    assert len(providers) == 26 and len(mains) == 24 and collections.Counter(i['case'] for i in mains) == expected
    assert all(i['valid'] is True and i['request']['model'] == 'gpt-fixture-model-1' for i in providers)
    assert len([i for i in protocol if i['kind'] == 'provider_completed']) == 26
    assert collections.Counter(i['case'] for i in protocol if i['kind'] == 'fixture_tool_call') == {name: 1 for name in expected}
    exits = [i for i in protocol if i['kind'] == 'exit']
    assert exits == [{'kind': 'exit', 'generation': n, 'code': 0, 'termination': 'natural'} for n in (0, 1)]
    snapshots = [i['snapshot'] for i in checks['checks'] if 'snapshot' in i]
    assert len(snapshots) == 34 and snapshots == [i for i in protocol if i['kind'] == 'permission_snapshot']
    for case in ('once', 'reject', 'always', 'mcp', 'childroot', 'read', 'shell', 'glob', 'url'):
        p = stage(case + '-pause-proof')
        assert p['files_unchanged'] is True and p['no_extra_provider_requests'] is True and p['no_started'] is True
    assert stage('once-newhome-pause-proof')['files_unchanged'] is True
    assert stage('once-proof')['files_unchanged'] is False and stage('once-proof')['grants'] == []
    assert stage('reject-proof')['files_unchanged'] is True and stage('reject-proof')['grants'] == []
    grant = stage('always-proof')['grants']
    assert len(grant) == 1 and stage('always-proof')['files_unchanged'] is False
    assert all(stage(n)['effect_pass'] is True and stage(n)['grants'] == grant for n in ('restart-proof', 'mixed-proof'))
    assert grant[0]['action'] == ('apply_patch' if side == 'oc' else 'edit')
    assert grant[0].get('pattern', grant[0].get('resource')) == ('approval.txt' if side == 'oc' else '*')
    assert stage('mcp-pause-proof')['mcp_call_count'] == 0 and stage('mcp-proof')['mcp_call_count'] == 1
    assert stage('childroot-proof')['mcp_call_count'] == 1 and stage('childroot-proof')['files_unchanged'] is True
    assert stage('child-feedback-visibility-proof')['visible'] == 'VIS36-FEEDBACK: do not read; continue with this correction.'
    assert stage('read-proof')['files_unchanged'] is True and stage('read-proof')['shell_effect'] is False
    assert stage('shell-proof')['files_unchanged'] is False and stage('shell-proof')['shell_effect'] is True
    assert all(stage(n)['files_unchanged'] is True for n in ('glob-proof', 'url-proof'))
    assert stage('autoonce-proof')['no_saved_grants'] is True and stage('autoonce-proof')['files_unchanged'] is True
    assert stage('settings-auto-config')['snapshot']['cli']['session']['permissions'] == 'autoaccept'
    final = stage('settings-final-config')['snapshot']
    assert final['cli']['session']['permissions'] == 'prompt'
    observations = [o['data'] for o in final['observations']]
    assert [g for d in observations for g in d.get('permission_grants', d.get('permission', []))] == grant
    return dict(provider_requests=26, provider_completions=26, main_requests=24, title_requests=2, tool_calls=14,
                readonly_snapshots=34, natural_exits=2, actual_grant=grant, pending_no_started_scope='producer SQLite snapshot heuristic, not CoreEvent/syscall trace')

def original_inventory():
    inventories = {}
    try:
        for profile in ('wide', 'small'):
            v.ROOTS = check.ROOTS = {'current': ROOTS['current-' + profile], 'current-logs': ROOTS['current-logs'],
                                     'current-driver': ROOTS['current-driver']}
            # Each separate archive keeps the unchanged 128 MiB/2,000-member guard.
            inventories[profile] = v.inventory()
    finally:
        v.ROOTS = check.ROOTS = ROOTS
    return inventories

def before():
    inventory = original_inventory()
    helpers = [Path(__file__), TMP / 'vis36-prompt-facts-r1.py', TMP / 'vis37-form-gate-20261012.py',
               TMP / 'vis37-form-independent-20261012-sole-verifier/owner_refinement.py',
               TMP / 'vis37-form-independent-20261012-sole-verifier/verifier.py',
               OUT / 'vis37-cancel-intermediate-audit.py', OUT / 'vis37-focus-paint-audit.py', *sorted(v.OLD_HELPERS.glob('*.py'))]
    older = [p for p in OUT.iterdir() if p.name.endswith(('.tar.xz', '.inventory.json', '.archived.json'))]
    stat_file = lambda p: dict(sha256=check.digest(p), metadata=check.metadata(p))
    result = dict(original_inventory=inventory, source_and_elf=source_and_elf(), helpers={str(p): stat_file(p) for p in helpers},
                  prior_proofs={str(p): stat_file(p) for p in older}, effects={role: {s: effects(ROOTS[role], s) for s in ('oc', 'upstream')}
                  for role in ('current-wide', 'current-small')})
    save(BEFORE, result)

def after():
    before = json.loads(BEFORE.read_bytes())
    assert original_inventory() == before['original_inventory'] and source_and_elf() == before['source_and_elf']
    pairs, inverses = [], {}
    for profile in ('wide', 'small'):
        role = 'current-' + profile
        originals = before['original_inventory'][profile]['files']
        path = OUT / ('vis36-prompt-' + profile + '-proof.tar.xz')
        receipt = json.loads(path.with_name(path.name.removesuffix('.tar.xz') + '.inventory.json').read_bytes())
        packed = path.read_bytes()
        check.require(hashlib.sha256(packed).hexdigest() == receipt['archive_sha256'], 'durable original archive hash')
        raw, xz = check.inverse_xz(packed)
        check.ROOTS = {'current': ROOTS[role], 'current-logs': ROOTS['current-logs'], 'current-driver': ROOTS['current-driver']}
        try:
            restored, tar = check.inverse_ustar(raw, originals)
        finally:
            check.ROOTS = ROOTS
        assert tar['inventory'] == receipt['inventory'] and check.sha(raw) == receipt['tar_sha256']
        inverses[profile] = {'xz': xz, 'tar': tar}
        del raw, restored
        root = ROOTS[role]
        lock = json.loads((root / 'capture.lock.json').read_bytes())
        assert [i['scenario'] for i in lock['attempts'] if i['mode'] == 'grid'] == ['permission-' + s for s in STAGES]
        for stage in STAGES:
            base = 'permission-' + stage
            paths = [root / side / (base + '.cells.json') for side in ('upstream', 'oc')]
            grid, _ = v.frames.compare_grid(*(check.load(p) for p in paths))
            png = v.frames.compare_png(root / 'upstream' / (base + '.png'), root / 'oc' / (base + '.png'))
            expected_grid = json.loads((root / (base + '.grid-diff.json')).read_bytes())
            expected_png = json.loads((root / (base + '.png-diff.json')).read_bytes())
            assert all(grid[k] == expected_grid[k] for k in grid if k != 'different_fields')
            assert all(png[k] == expected_png[k] for k in png)
            assert expected_grid['status'] == ('PASS' if grid['different_cells'] == 0 and not grid['cursor_differs'] else 'FAIL')
            assert expected_png['status'] == ('PASS' if png['different_pixels'] == 0 else 'FAIL')
            pairs.append(dict(profile=profile, stage=stage, grid=grid, png=png))
    assert original_inventory() == before['original_inventory'] and source_and_elf() == before['source_and_elf']
    assert all(dict(sha256=check.digest(p), metadata=check.metadata(p)) == record
               for group in ('helpers', 'prior_proofs') for p, record in before[group].items())
    totals = dict(pairs=len(pairs), pngs=2 * len(pairs), styled_cells=sum(p['grid']['cells_checked'] for p in pairs),
                  differing_cells=sum(p['grid']['different_cells'] for p in pairs), pixels=sum(p['png']['pixels_checked'] for p in pairs),
                  differing_pixels=sum(p['png']['different_pixels'] for p in pairs), cursor_pairs_equal=sum(not p['grid']['cursor_differs'] for p in pairs),
                  whole_grids_equal=sum(p['grid']['different_cells'] == 0 and not p['grid']['cursor_differs'] for p in pairs),
                  whole_pngs_equal=sum(p['png']['different_pixels'] == 0 for p in pairs))
    save(AFTER, dict(status='INDEPENDENT_CURRENT_PROMPT_FULL_BYTE_EXACT_RECOUNT_T44_ACTIVE_NOT_PASS', inverses=inverses,
                     totals=totals, pairs=pairs, effects=before['effects'], preserved_prior_proofs=len(before['prior_proofs']),
                     source_and_elf=before['source_and_elf'], memory=check.proc_memory(), quota=check.quota()))

if __name__ == '__main__':
    {'before': before, 'after': after}[sys.argv[1]]()
