"""Independent Subagent glyph/layout roles; every original byte and whole frame."""
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
BASE = '70ad2d3b3b155d6ce4cc75f982e88e711a7c8b05'
ROOTS = {'current-wide':TMP/'vis36-mode-current-010/cli-wide-r10',
         'current-small':TMP/'vis36-mode-current-010/cli-small-r10',
         'current-logs':TMP/'vis36-mode-current-010', 'current-driver':TMP/'vis36-mode-capture-r10.py'}
v.ROOTS = check.ROOTS = ROOTS
check.HEAD = v.frames.HEAD = BASE
STAGES = ('home auto-once-completed auto-shell-completed auto-mixed-completed settings-prompt '
          'settings-autoaccept settings-auto-first-escape auto-autoonce-completed settings-palette-filtered '
          'settings-auto-reopen settings-restored-prompt settings-prompt-first-escape auto-promptagain-completed '
          'auto-childroot-completed cli-restart auto-clirestart-completed').split()
selected = [(OUT/'vis37-cancel-intermediate-audit.py', {'save'}), (OUT/'vis36-auto-audit.py', {'effects'}),
            (OUT/'vis37-focus-paint-audit.py', {'strict_commands'}),
            (OUT/'vis36-dialog-intermediate-audit.py', {'modal_geometry', 'profile_effects'}),
            (OUT/'vis36-patch-failure-audit.py', {'failed_patch'})]
for path, names in selected:
    tree = ast.parse(check.read_bytes(path), filename=str(path))
    functions = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name in names]
    check.require({n.name for n in functions} == names, 'only sealed independent functions, not old entrypoints')
    if names == {'strict_commands'}:
        counts = [n for n in ast.walk(functions[0]) if isinstance(n, ast.Constant) and n.value == 36]
        check.require(len(counts) == 1, 'one known Question count, no validation changes')
        counts[0].value = 2 * len(STAGES)
    exec(compile(ast.Module(body=functions, type_ignores=[]), str(path), 'exec'), globals())
BEFORE, AFTER = OUT/'vis36-subagent-gap-before.json', OUT/'vis36-subagent-gap-after.json'

def source_and_elf():
    source = v.source()
    old_root = TMP/'vis36-mode-current-009/cli-wide-r9'
    prior, old = (check.load(old_root/n) for n in ('source-manifest.json', 'capture.lock.json'))
    changed = sorted(n for n in source if prior.get(n) != source[n])
    check.require(len(source) == 713 and set(source) == set(prior) and changed == ['crates/oc-tui/src/tools.rs'],
                  'only scoped Subagent layout owner and its existing inline test changed')
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

def subagent_header(grids):
    label = 'VIS36 child permission feedback'
    positions, cells = {}, {}
    for side, grid in grids.items():
        rows = [''.join(c['symbol'] for c in row) for row in grid['cells']]
        matches = [y for y, text in enumerate(rows) if label in text and '✓' in text]
        check.require(len(matches) == 1, 'unique actual completed child header')
        y = matches[0]
        x = rows[y].index(label) - 2
        span = grid['cells'][y][x:x+2+len(label)]
        check.require(span[0]['symbol'] == '✓' and span[1]['symbol'] == ' ' and not span[1]['modifiers'], 'glyph and layout spacer are separate roles')
        check.require(span[0]['fg'] == span[2]['fg'], 'completed glyph and label retain their muted semantic role')
        check.require(span[1]['fg'] == grid['cells'][y][x-1]['fg'], 'layout blank inherits adjacent canvas foreground')
        positions[side], cells[side] = dict(x=x, y=y), span
    check.require(positions['oc'] == positions['upstream'] and cells['oc'] == cells['upstream'], 'complete paired child header cells including layout spacer match')
    return dict(positions=positions, checked_header_cells=len(cells['oc']), full_styled_header_equal=True,
                scope='Named role diagnostic only; every whole-frame styled cell/cursor/pixel is compared separately without masks.')

check.require(os.getuid() == 1003 and check.git('rev-parse', 'HEAD').decode().strip() == BASE, 'dedicated owner/current base')
helpers = {str(p):check.digest(p) for p in [Path(__file__), HERE/'verifier.py', HERE/'owner_refinement.py',
           *(p for p, _ in selected), *sorted(v.OLD_HELPERS.glob('*.py'))]}
originals, source = v.inventory(), source_and_elf()
if sys.argv[1:] == ['before']:
    prior = {str(p):dict(sha256=check.digest(p), metadata=check.metadata(p)) for p in sorted(OUT.iterdir())
             if p.name.endswith(('.tar.xz', '.inventory.json', '.archived.json'))}
    save(BEFORE, dict(status='INDEPENDENT_VIS39_SUBAGENT_GAP_SEALED_BEFORE', originals=originals, source=source, helpers=helpers, prior=prior))
elif sys.argv[1:] == ['after']:
    before = check.load(BEFORE)
    check.require(originals == before['originals'] and source == before['source'] and helpers == before['helpers'], 'sealed originals/source/helpers unchanged')
    archive, receipt = OUT/'vis36-subagent-gap-proof.tar.xz', check.load(OUT/'vis36-subagent-gap-proof.inventory.json')
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
            if stage == 'auto-mixed-completed':
                row['patch_roles'] = failed_patch(grids)
            if stage == 'auto-childroot-completed':
                row['subagent_header'] = subagent_header(grids)
            pairs.append(row)
        facts[prefix] = profile_effects(prefix)
    check.require(v.inventory() == originals and source_and_elf() == source and all(check.digest(p) == s for p,s in helpers.items()) and
                  all(check.digest(p) == r['sha256'] and check.metadata(p) == r['metadata'] for p,r in before['prior'].items()), 'all original metadata/source/ELFs/helpers/prior proofs unchanged')
    totals = dict(paired_frames=len(pairs), whole_pngs=2*len(pairs), checked_cells=sum(p['grid']['cells_checked'] for p in pairs),
                  checked_pixels=sum(p['png']['pixels_checked'] for p in pairs), different_cells=sum(p['grid']['different_cells'] for p in pairs),
                  different_pixels=sum(p['png']['different_pixels'] for p in pairs), equal_cursor_pairs=sum(not p['grid']['cursor_differs'] for p in pairs),
                  equal_full_modals=sum('modal' in p for p in pairs), equal_patch_role_frames=sum('patch_roles' in p for p in pairs),
                  equal_subagent_header_frames=sum('subagent_header' in p for p in pairs))
    save(AFTER, dict(status='INDEPENDENT_VIS39_SUBAGENT_GAP_FULL_BYTE_EXACT_T44_ACTIVE_NOT_PASS', source=source, helpers=helpers, totals=totals,
                     inverse=inverse['inventory'], xz=xz, pairs=pairs, effects=facts, preserved_prior_proofs=len(before['prior']),
                     scope='All whole frames retained/recounted. Glyph unchanged; layout blank is not text. Frozen Shell success override and raw Patch denial copy intact. Full VIS39/VIS36/T44 NOT_PASS; SQLRO is not CoreEvent/syscall evidence.'))
else:
    raise SystemExit('before or after; existing reports immutable')
