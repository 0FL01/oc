"""Independent Settings-filter full-frame inverse; no producer codec imports."""
import sys
sys.dont_write_bytecode = True
import ast
import collections
import importlib.util
import json
import os
from pathlib import Path
import resource

resource.setrlimit(resource.RLIMIT_AS, (384 * 1024**2, 384 * 1024**2))
REPO, TMP = Path('/home/opencode/ai/oc'), Path('/home/opencode/.cache/opencode-tmp/opencode')
OUT = REPO/'evidence/tui'
HERE = TMP/'vis37-form-independent-20261012-sole-verifier'
spec = importlib.util.spec_from_file_location('sealed_manual_observer', HERE/'owner_refinement.py')
observer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(observer)
v, check = observer.v, observer.v.inverse
check.require('vis36-' not in check.PREFIXES, 'known older additive quota scope')
check.PREFIXES = (*check.PREFIXES, 'vis36-')
BASE = 'a8d7f0483c64036efe1f5bc1be5531f4f81a36c3'
ROOTS = {'current':TMP/'vis36-mode-current-004/cli-wide-r4',
         'current-logs':TMP/'vis36-mode-current-004',
         'current-driver':TMP/'vis36-mode-capture-r4.py'}
v.ROOTS = check.ROOTS = ROOTS
check.HEAD = v.frames.HEAD = BASE
STAGES = ('home auto-once-completed auto-shell-completed auto-mixed-completed settings-prompt '
          'settings-autoaccept settings-auto-first-escape auto-autoonce-completed settings-palette-filtered '
          'settings-auto-reopen settings-restored-prompt settings-prompt-first-escape auto-promptagain-completed '
          'auto-childroot-completed cli-restart auto-clirestart-completed').split()
for path, names in [(OUT/'vis37-cancel-intermediate-audit.py', {'save'}), (OUT/'vis36-auto-audit.py', {'effects'})]:
    tree = ast.parse(check.read_bytes(path), filename=str(path))
    functions = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name in names]
    check.require({n.name for n in functions} == names, 'only sealed functions, not old entrypoints')
    exec(compile(ast.Module(body=functions, type_ignores=[]), str(path), 'exec'), globals())
BEFORE, AFTER = OUT/'vis36-settings-before.json', OUT/'vis36-settings-after.json'

def source_and_elf():
    source = v.source()
    prior_root = TMP/'vis36-mode-current-003/cli-wide-r3'
    prior = check.load(prior_root/'source-manifest.json')
    changed = sorted(name for name in source if prior.get(name) != source[name])
    check.require(len(source) == 713 and set(source) == set(prior) and changed == [
        'crates/oc-tui/src/app/input.rs', 'crates/oc-tui/src/app/tests/input.rs', 'crates/oc/src/approval_tests.rs'],
        'only Settings ACK owner and relevant tests changed')
    ep = ROOTS['current']
    lock, manifest, commands = (check.load(ep/name) for name in ('capture.lock.json', 'source-manifest.json', 'commands.json'))
    check.require(manifest == source and check.producer_sha(source) == lock['oc']['source_manifest_sha256'] and lock['oc']['commit'] == BASE,
                  'current713 inputs and actual captured producer identity')
    builds = [item for item in commands if item['argv'][0] == 'cargo']
    check.require(len(builds) == 1 and builds[0]['argv'] == ['cargo', 'build', '--locked'] and builds[0]['exit_code'] == 0, 'one captured locked build')
    argv = commands[0]['argv']
    pairs = list(zip(argv[2::2], argv[3::2], strict=True))
    check.require(argv[1] == str(REPO/'scripts/tui_capture/capture.mjs') and len(pairs) == len(dict(pairs)) and dict(pairs) == {
        '--reference':str(TMP/'t44-reference/package/bin/opencode'), '--oc':str(REPO/'target/debug/oc'), '--build-oc':'true',
        '--geometry':'true', '--sample':'short', '--sidebar':'hide', '--permission':'true', '--permission-mode':'auto-cli',
        '--columns':'121', '--rows':'40', '--output':str(ep)}, 'exact actual invocation, no alternate profile/mask/tolerance')
    elf = {side:v.frames.elf(Path(lock[side]['executable_path'])) for side in ('oc', 'upstream')}
    old = check.load(prior_root/'capture.lock.json')
    check.require(all(elf[s]['sha256'] == lock[s]['executable_sha256'] and
                      ((elf[s]['sha256'] == old[s]['executable_sha256']) == (s == 'upstream')) for s in elf), 'rebuilt native and same pinned donor')
    return dict(source_manifest_sha256=check.producer_sha(source), changed_inputs=changed, elf=elf)

def settings_filter(grids, stage):
    positions, spans = {}, {}
    for side, grid in grids.items():
        rows = [''.join(cell['symbol'] for cell in row) for row in grid['cells']]
        query_rows = [y for y, text in enumerate(rows) if 'Permissions' in text]
        check.require(len(query_rows) == 2, 'actual query plus selected Permissions option')
        y = query_rows[0]
        x = rows[y].index('Permissions')
        spans[side] = grid['cells'][y][x:x+len('Permissions')]
        positions[side] = dict(x=x, y=y, cursor=grid['cursor'])
        check.require('auto accept' in rows[query_rows[1]] if stage == 'settings-autoaccept' else 'prompt' in rows[query_rows[1]],
                      'actual saved preference appears on selected row')
    check.require(spans['oc'] == spans['upstream'], 'all query glyph styles match actual focused filter')
    return dict(query='Permissions', positions=positions, full_query_styled_cells_equal=True,
                geometry_equal=positions['oc'] == positions['upstream'], diagnostic_only_no_crop_or_mask=True)

check.require(os.getuid() == 1003 and check.git('rev-parse', 'HEAD').decode().strip() == BASE, 'dedicated owner/current base')
helpers = {str(p):check.digest(p) for p in [Path(__file__), HERE/'verifier.py', HERE/'owner_refinement.py',
           OUT/'vis37-cancel-intermediate-audit.py', OUT/'vis36-auto-audit.py', *sorted(v.OLD_HELPERS.glob('*.py'))]}
originals, source = v.inventory(), source_and_elf()
if sys.argv[1:] == ['before']:
    prior = {str(p):dict(sha256=check.digest(p), metadata=check.metadata(p)) for p in sorted(OUT.iterdir())
             if p.name.endswith(('.tar.xz', '.inventory.json', '.archived.json'))}
    save(BEFORE, dict(status='INDEPENDENT_VIS36_SETTINGS_SEALED_BEFORE', originals=originals, source=source, helpers=helpers, prior=prior))
elif sys.argv[1:] == ['after']:
    before = check.load(BEFORE)
    check.require(originals == before['originals'] and source == before['source'] and helpers == before['helpers'], 'all sealed originals/source/helpers unchanged')
    archive, receipt = OUT/'vis36-settings-proof.tar.xz', check.load(OUT/'vis36-settings-proof.inventory.json')
    raw, xz = check.inverse_xz(check.read_bytes(archive))
    restored, inverse = check.inverse_ustar(raw, originals['files'])
    check.require(check.digest(archive) == receipt['archive_sha256'] and check.sha(raw) == receipt['tar_sha256'] and inverse['inventory'] == receipt['inventory'],
                  'manual full single-XZ/canonical-USTAR inverse')
    del raw, restored
    ep, lock = ROOTS['current'], check.load(ROOTS['current']/'capture.lock.json')
    check.require([item['scenario'] for item in lock['attempts'] if item['mode'] == 'grid'] == ['permission-'+s for s in STAGES], 'every actual full frame retained')
    pairs = []
    for stage in STAGES:
        name = 'permission-'+stage
        grids = {s:check.load(ep/s/(name+'.cells.json')) for s in ('oc', 'upstream')}
        grid, _ = v.frames.compare_grid(grids['upstream'], grids['oc'])
        png = v.frames.compare_png(ep/'upstream'/(name+'.png'), ep/'oc'/(name+'.png'))
        for mode, metric in (('grid', grid), ('png', png)):
            recorded = check.load(ep/(name+'.'+mode+'-diff.json'))
            check.require(all(recorded[k] == metric[k] for k in metric if k != 'different_fields'), 'independent complete field/cursor/pixel recount')
            equal = (grid['different_cells'] == 0 and not grid['cursor_differs']) if mode == 'grid' else png['different_pixels'] == 0
            check.require(recorded['status'] == ('PASS' if equal else 'FAIL'), 'strict actual result without normalization')
        row = dict(stage=stage, grid=grid, png=png)
        if stage in ('settings-autoaccept', 'settings-restored-prompt'):
            row['settings_filter'] = settings_filter(grids, stage)
        pairs.append(row)
    facts = {s:effects(s) for s in ('oc', 'upstream')}
    check.require(v.inventory() == originals and source_and_elf() == source and all(check.digest(p) == s for p, s in helpers.items()) and
                  all(check.digest(p) == r['sha256'] and check.metadata(p) == r['metadata'] for p, r in before['prior'].items()), 'all original/meta/source/ELFs/helpers/prior proof bytes unchanged')
    totals = dict(paired_frames=len(pairs), whole_pngs=2*len(pairs), checked_cells=sum(p['grid']['cells_checked'] for p in pairs),
                  checked_pixels=sum(p['png']['pixels_checked'] for p in pairs), different_cells=sum(p['grid']['different_cells'] for p in pairs),
                  different_pixels=sum(p['png']['different_pixels'] for p in pairs), equal_cursor_pairs=sum(not p['grid']['cursor_differs'] for p in pairs))
    save(AFTER, dict(status='INDEPENDENT_VIS36_SETTINGS_FULL_BYTE_EXACT_AND_RECOUNT_T44_ACTIVE_NOT_PASS', source=source, helpers=helpers, totals=totals,
                     inverse=inverse['inventory'], xz=xz, pairs=pairs, effects=facts, preserved_prior_proofs=len(before['prior']),
                     scope='Query retention and live mode/root/child lifecycle only; odd-width dialog geometry remains separate. SQLite is not a syscall/CoreEvent trace; whole VIS36/T44 NOT_PASS.'))
else:
    raise SystemExit('before or after; existing reports immutable')
