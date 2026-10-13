"""Independent healthy Settings projection inverse/recount; all full frames retained."""
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
OUT, HERE = REPO/'evidence/tui', TMP/'vis37-form-independent-20261012-sole-verifier'
spec = importlib.util.spec_from_file_location('sealed_manual_observer', HERE/'owner_refinement.py')
observer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(observer)
v, check = observer.v, observer.v.inverse
check.require('vis36-' not in check.PREFIXES, 'known older additive quota scope')
check.PREFIXES = (*check.PREFIXES, 'vis36-')
BASE = '90583174d4f0d6730de77149ae652ff0a695dc5b'
ROOTS = {'current-wide':TMP/'vis36-mode-current-007/cli-wide-r7',
         'current-small':TMP/'vis36-mode-current-007/cli-small-r7',
         'current-logs':TMP/'vis36-mode-current-007', 'current-driver':TMP/'vis36-mode-capture-r7.py'}
v.ROOTS = check.ROOTS = ROOTS
check.HEAD = v.frames.HEAD = BASE
STAGES = ('home auto-once-completed auto-shell-completed auto-mixed-completed settings-prompt '
          'settings-autoaccept settings-auto-first-escape auto-autoonce-completed settings-palette-filtered '
          'settings-auto-reopen settings-restored-prompt settings-prompt-first-escape auto-promptagain-completed '
          'auto-childroot-completed cli-restart auto-clirestart-completed').split()
selected = [(OUT/'vis37-cancel-intermediate-audit.py', {'save'}), (OUT/'vis36-auto-audit.py', {'effects'}),
            (OUT/'vis37-focus-paint-audit.py', {'strict_commands'}),
            (OUT/'vis36-dialog-intermediate-audit.py', {'modal_geometry', 'profile_effects'})]
for path, names in selected:
    tree = ast.parse(check.read_bytes(path), filename=str(path))
    functions = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name in names]
    check.require({n.name for n in functions} == names, 'only sealed independent functions, not old entrypoints')
    if names == {'strict_commands'}:
        counts = [n for n in ast.walk(functions[0]) if isinstance(n, ast.Constant) and n.value == 36]
        check.require(len(counts) == 1, 'one known Question count, no validation changes')
        counts[0].value = 2 * len(STAGES)
    exec(compile(ast.Module(body=functions, type_ignores=[]), str(path), 'exec'), globals())
BEFORE, AFTER = OUT/'vis36-settings-clean-before.json', OUT/'vis36-settings-clean-after.json'

def source_and_elf():
    source = v.source()
    old_root = TMP/'vis36-mode-current-006/cli-wide-r6'
    prior, old = (check.load(old_root/n) for n in ('source-manifest.json', 'capture.lock.json'))
    changed = sorted(n for n in source if prior.get(n) != source[n])
    check.require(len(source) == 713 and set(source) == set(prior) and changed == [
        'crates/oc-tui/src/app/input.rs', 'crates/oc-tui/src/app/tests/input.rs', 'crates/oc/src/approval_tests.rs'],
        'only healthy Settings projection and its real consumers changed')
    elf = None
    for prefix, columns in [('current-wide', 121), ('current-small', 79)]:
        ep = ROOTS[prefix]
        lock, manifest, commands = (check.load(ep/n) for n in ('capture.lock.json', 'source-manifest.json', 'commands.json'))
        check.require(manifest == source and check.producer_sha(source) == lock['oc']['source_manifest_sha256'] and lock['oc']['commit'] == BASE,
                      'current713 inputs and captured producer identity')
        builds = [i for i in commands if i['argv'][0] == 'cargo']
        check.require(len(builds) == 1 and builds[0]['argv'] == ['cargo', 'build', '--locked'] and builds[0]['exit_code'] == 0, 'one captured locked build')
        argv = commands[0]['argv']
        pairs = list(zip(argv[2::2], argv[3::2], strict=True))
        check.require(argv[1] == str(REPO/'scripts/tui_capture/capture.mjs') and len(pairs) == len(dict(pairs)) and dict(pairs) == {
            '--reference':str(TMP/'t44-reference/package/bin/opencode'), '--oc':str(REPO/'target/debug/oc'), '--build-oc':'true',
            '--geometry':'true', '--sample':'short', '--sidebar':'hide', '--permission':'true', '--permission-mode':'auto-cli',
            '--columns':str(columns), '--rows':'40', '--output':str(ep)}, 'exact invocation without alternate profile or normalization')
        strict_commands(ep, lock, commands)
        actual = {s:v.frames.elf(Path(lock[s]['executable_path'])) for s in ('oc', 'upstream')}
        check.require(all(actual[s]['sha256'] == lock[s]['executable_sha256'] and
                      ((actual[s]['sha256'] == old[s]['executable_sha256']) == (s == 'upstream')) for s in actual), 'rebuilt native and same pinned donor')
        check.require(elf is None or actual == elf, 'same actual executables in both profiles')
        elf = actual
    return dict(source_manifest_sha256=check.producer_sha(source), changed_inputs=changed, elf=elf)

check.require(os.getuid() == 1003 and check.git('rev-parse', 'HEAD').decode().strip() == BASE, 'dedicated owner/current base')
helpers = {str(p):check.digest(p) for p in [Path(__file__), HERE/'verifier.py', HERE/'owner_refinement.py',
           *(p for p, _ in selected), *sorted(v.OLD_HELPERS.glob('*.py'))]}
originals, source = v.inventory(), source_and_elf()
if sys.argv[1:] == ['before']:
    prior = {str(p):dict(sha256=check.digest(p), metadata=check.metadata(p)) for p in sorted(OUT.iterdir())
             if p.name.endswith(('.tar.xz', '.inventory.json', '.archived.json'))}
    save(BEFORE, dict(status='INDEPENDENT_VIS36_SETTINGS_CLEAN_SEALED_BEFORE', originals=originals, source=source, helpers=helpers, prior=prior))
elif sys.argv[1:] == ['after']:
    before = check.load(BEFORE)
    check.require(originals == before['originals'] and source == before['source'] and helpers == before['helpers'], 'sealed originals/source/helpers unchanged')
    archive, receipt = OUT/'vis36-settings-clean-proof.tar.xz', check.load(OUT/'vis36-settings-clean-proof.inventory.json')
    raw, xz = check.inverse_xz(check.read_bytes(archive))
    restored, inverse = check.inverse_ustar(raw, originals['files'])
    check.require(check.digest(archive) == receipt['archive_sha256'] and check.sha(raw) == receipt['tar_sha256'] and inverse['inventory'] == receipt['inventory'], 'manual full single-XZ/canonical-USTAR byte inverse')
    del raw, restored
    pairs, facts = [], {}
    for prefix in ('current-wide', 'current-small'):
        ep, lock = ROOTS[prefix], check.load(ROOTS[prefix]/'capture.lock.json')
        check.require([i['scenario'] for i in lock['attempts'] if i['mode'] == 'grid'] == ['permission-'+s for s in STAGES], 'every actual full frame retained')
        for stage in STAGES:
            name = 'permission-'+stage
            grids = {s:check.load(ep/s/(name+'.cells.json')) for s in ('oc', 'upstream')}
            grid, _ = v.frames.compare_grid(grids['upstream'], grids['oc'])
            png = v.frames.compare_png(ep/'upstream'/(name+'.png'), ep/'oc'/(name+'.png'))
            for mode, metric in [('grid', grid), ('png', png)]:
                recorded = check.load(ep/(name+'.'+mode+'-diff.json'))
                check.require(all(recorded[k] == metric[k] for k in metric if k != 'different_fields'), 'independent whole field/cursor/pixel recount')
                equal = grid['different_cells'] == 0 and not grid['cursor_differs'] if mode == 'grid' else png['different_pixels'] == 0
                check.require(recorded['status'] == ('PASS' if equal else 'FAIL'), 'strict result without normalization')
            row = dict(profile=prefix, stage=stage, grid=grid, png=png)
            if stage in ('settings-autoaccept', 'settings-restored-prompt'):
                row['modal'] = modal_geometry(grids, stage)
                check.require(not row['modal']['different_modal_cells'], 'all600 actual filtered modal cells still match')
            if stage.startswith('settings-'):
                rows = [''.join(c['symbol'] for c in r) for r in grids['oc']['cells']]
                check.require(not any('Provider request' in text or text.strip() == 'Services' for text in rows), 'no healthy permanent provider-summary row')
                row['healthy_summary_absent'] = True
            pairs.append(row)
        facts[prefix] = profile_effects(prefix)
    check.require(v.inventory() == originals and source_and_elf() == source and all(check.digest(p) == s for p, s in helpers.items()) and
                  all(check.digest(p) == r['sha256'] and check.metadata(p) == r['metadata'] for p, r in before['prior'].items()), 'all original metadata/source/ELFs/helpers/prior proofs unchanged')
    totals = dict(paired_frames=len(pairs), whole_pngs=2*len(pairs), checked_cells=sum(p['grid']['cells_checked'] for p in pairs),
                  checked_pixels=sum(p['png']['pixels_checked'] for p in pairs), different_cells=sum(p['grid']['different_cells'] for p in pairs),
                  different_pixels=sum(p['png']['different_pixels'] for p in pairs), equal_cursor_pairs=sum(not p['grid']['cursor_differs'] for p in pairs),
                  equal_full_modals=sum('modal' in p for p in pairs), healthy_settings_frames=sum('healthy_summary_absent' in p for p in pairs))
    save(AFTER, dict(status='INDEPENDENT_VIS36_SETTINGS_CLEAN_FULL_BYTE_EXACT_T44_ACTIVE_NOT_PASS', source=source, helpers=helpers, totals=totals,
                     inverse=inverse['inventory'], xz=xz, pairs=pairs, effects=facts, preserved_prior_proofs=len(before['prior']),
                     scope='All full frames retained/recounted. Healthy summary hidden; typed real issues retain Settings detail routes. Shell success suppression is frozen VIS16 override; full VIS36/T44 NOT_PASS. SQLRO is not CoreEvent/syscall evidence.'))
else:
    raise SystemExit('before or after; existing reports immutable')
