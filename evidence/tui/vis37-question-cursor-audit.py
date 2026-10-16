"""Independent current012 full inverse/recount; focused Question caret color."""
import sys
sys.dont_write_bytecode = True
import ast
import importlib.util
import json
import os
from pathlib import Path
import resource

resource.setrlimit(resource.RLIMIT_AS, (384 * 1024**2, 384 * 1024**2))
REPO = Path('/home/opencode/ai/oc')
TMP = Path('/home/opencode/.cache/opencode-tmp/opencode')
OUT = REPO/'evidence/tui'
HERE = TMP/'vis37-form-independent-20261012-sole-verifier'
spec = importlib.util.spec_from_file_location('immutable_question_observer', HERE/'owner_refinement.py')
observer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(observer)
v, check = observer.v, observer.v.inverse
BASE = 'eceb08c3247d4fded43394eba22896337ff638ad'
v.ROOTS = {**{'current-'+p:TMP/'vis37-form-current-012'/p for p in ('small','wide','tall')},
           'current-logs':TMP/'vis37-form-current-012','current-driver':TMP/'vis37-form-capture-r12.py'}
check.ROOTS = v.ROOTS
check.HEAD = v.frames.HEAD = BASE
STAGES = list(check.STAGES)
STAGES.insert(3,'question-multiple-focused')
READER = OUT/'vis37-focus-paint-audit.py'
INTERMEDIATE = OUT/'vis37-cancel-intermediate-audit.py'
PADDING = OUT/'vis37-tab-padding-audit.py'
COLORS = OUT/'vis37-tab-color-final-audit.py'
STATUS = OUT/'vis37-question-status-audit.py'
for path,names in ((READER,('strict_commands','effects')),(INTERMEDIATE,('save','cancel_row')),
                   (PADDING,('header_padding',)),(COLORS,('full_header',)),(STATUS,('small_underlay',))):
    tree = ast.parse(check.read_bytes(path), filename=str(path))
    functions = [n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in names]
    check.require(len(functions)==len(names),'exact delivered independent functions; no producer codec or old entrypoint')
    exec(compile(ast.Module(body=functions,type_ignores=[]),str(path),'exec'),globals())
BEFORE,AFTER = OUT/'vis37-question-cursor-before.json',OUT/'vis37-question-cursor-after.json'

def source_and_elf():
    source = v.source()
    prior = check.load(TMP/'vis37-form-current-011/wide/source-manifest.json')
    changed = sorted(n for n in source if prior.get(n)!=source[n])
    check.require(len(source)==713 and set(source)==set(prior) and changed==[
        'crates/oc-tui/src/app/tests/input.rs','crates/oc-tui/src/question_view.rs','crates/oc-tui/src/views.rs'],
        'only bound custom editor, cursor-color owner and nearest regression changed')
    elf = {}
    for profile in ('small','wide','tall'):
        ep = v.ROOTS['current-'+profile]
        lock,manifest,commands = (check.load(ep/n) for n in ('capture.lock.json','source-manifest.json','commands.json'))
        check.require(manifest==source and check.producer_sha(source)==lock['oc']['source_manifest_sha256'] and lock['oc']['commit']==BASE,
                      'actual713 source and unchanged recorded producer identity')
        builds = [c for c in commands if c['argv'][0]=='cargo']
        check.require(len(builds)==1 and builds[0]['argv']==['cargo','build','--locked'] and builds[0]['exit_code']==0,'one exact acquisition build')
        strict_commands(ep,lock,commands)
        old = check.load(TMP/'vis37-form-current-011'/profile/'capture.lock.json')
        for side in ('oc','upstream'):
            actual = v.frames.elf(Path(lock[side]['executable_path']))
            check.require(actual['sha256']==lock[side]['executable_sha256'] and
                          ((actual['sha256']==old[side]['executable_sha256'])==(side=='upstream')),'rebuilt native and unchanged pinned donor')
            elf[side] = actual
    return dict(source_manifest_sha256=check.producer_sha(source),changed_inputs=changed,elf=elf)

check.require(os.getuid()==1003 and check.git('rev-parse','HEAD').decode().strip()==BASE,'dedicated owner and actual base')
helpers = {str(p):check.digest(p) for p in [Path(__file__),READER,INTERMEDIATE,PADDING,COLORS,STATUS,HERE/'verifier.py',HERE/'owner_refinement.py',*sorted(v.OLD_HELPERS.glob('*.py'))]}
originals,source = v.inventory(),source_and_elf()
if sys.argv[1:]==['before']:
    prior = {str(p):dict(sha256=check.digest(p),metadata=check.metadata(p)) for p in sorted(OUT.iterdir())
             if p.name.endswith(('.tar.xz','.inventory.json','.archived.json'))}
    save(BEFORE,dict(status='INDEPENDENT_CURRENT012_SEALED_BEFORE',originals=originals,source=source,helpers=helpers,prior=prior))
elif sys.argv[1:]==['after']:
    before = check.load(BEFORE)
    check.require(originals==before['originals'] and source==before['source'] and helpers==before['helpers'],'all before identities retained')
    archive,receipt_path = OUT/'vis37-form-r12-proof.tar.xz',OUT/'vis37-form-r12-proof.inventory.json'
    receipt = check.load(receipt_path)
    raw,xz = check.inverse_xz(check.read_bytes(archive))
    restored,inverse = check.inverse_ustar(raw,originals['files'])
    check.require(check.digest(archive)==receipt['archive_sha256'] and check.sha(raw)==receipt['tar_sha256'] and inverse['inventory']==receipt['inventory'],
                  'manual canonical XZ/USTAR every original byte and direct same-run/side alias')
    del restored,raw
    pairs,facts = [],{}
    for profile in ('small','wide','tall'):
        ep = v.ROOTS['current-'+profile]
        facts.update({profile+'-'+s:effects(ep,s) for s in ('oc','upstream')})
        for stage in STAGES:
            name = 'tool-preview-'+stage
            grids = {s:check.load(ep/s/(name+'.cells.json')) for s in ('oc','upstream')}
            metric,_ = v.frames.compare_grid(grids['upstream'],grids['oc'])
            png = v.frames.compare_png(ep/'upstream'/(name+'.png'),ep/'oc'/(name+'.png'))
            for mode,value in (('grid',metric),('png',png)):
                recorded = check.load(ep/(name+'.'+mode+'-diff.json'))
                check.require(all(recorded[k]==value[k] for k in value if k!='different_fields'),'actual independent whole-frame recount')
                equal = (metric['different_cells']==0 and not metric['cursor_differs']) if mode=='grid' else png['different_pixels']==0
                check.require(recorded['status']==('PASS' if equal else 'FAIL'),'exact comparator status without tolerance')
            pair = dict(profile=profile,stage=stage,grid=metric,png=png)
            if stage!='question-home': pair['header'] = full_header(grids)
            if stage in STAGES[1:9]+['question-cancel-pending']: pair['pending_owner'] = v.owner(grids,'Questions')
            if stage=='question-answered-card': pair['answered_owner'] = v.owner(grids,'# Questions')
            if profile=='small' and stage in ('question-long-field','question-custom-empty','question-custom-edited'):
                pair['small_underlay'] = small_underlay(grids)
            if stage in ('question-custom-empty','question-custom-edited'):
                check.require(metric['different_cells']==0 and not metric['cursor_differs'] and png['different_pixels']==0,
                              'complete custom-editor frame and caret raster match without masking or normalization')
                pair['custom_cursor_full_frame_equal'] = True
            if stage in ('question-dismissed','question-cancel-reopened'):
                row = cancel_row(grids)
                check.require(row['all_named_cells_equal'] and row['geometry']['oc']==row['geometry']['upstream'],'cancelled row every cell and geometry remain equal')
                pair['cancelled_row'] = row
            pairs.append(pair)
    totals = dict(paired_frames=len(pairs),whole_pngs=len(pairs)*2,checked_cells=sum(p['grid']['cells_checked'] for p in pairs),
                  checked_pixels=sum(p['png']['pixels_checked'] for p in pairs),different_cells=sum(p['grid']['different_cells'] for p in pairs),
                  different_pixels=sum(p['png']['different_pixels'] for p in pairs),equal_cursor_pairs=sum(not p['grid']['cursor_differs'] for p in pairs),
                  equal_whole_grid_pairs=sum(p['grid']['different_cells']==0 and not p['grid']['cursor_differs'] for p in pairs),
                  equal_whole_png_pairs=sum(p['png']['different_pixels']==0 for p in pairs),
                  pending_owner_equal_pairs=sum(p.get('pending_owner',{}).get('all_owner_styled_cells_equal',False) for p in pairs),
                  answered_owner_equal_pairs=sum(p.get('answered_owner',{}).get('all_owner_styled_cells_equal',False) for p in pairs),
                  cancelled_row_equal_pairs=sum(p.get('cancelled_row',{}).get('all_named_cells_equal',False) for p in pairs),
                  complete_header_equal_pairs=sum(p.get('header',{}).get('complete_header_equal',False) for p in pairs),
                  small_underlay_equal_pairs=sum(p.get('small_underlay',{}).get('all_underlay_styled_cells_equal',False) for p in pairs),
                  custom_cursor_full_frame_equal_pairs=sum(p.get('custom_cursor_full_frame_equal',False) for p in pairs))
    check.require(v.inventory()==originals and source_and_elf()==source and all(check.digest(p)==sha for p,sha in helpers.items()) and
                  all(check.digest(p)==r['sha256'] and check.metadata(p)==r['metadata'] for p,r in before['prior'].items()),'all original metadata/source/ELFs/helpers/prior proofs unchanged')
    save(AFTER,dict(status='INDEPENDENT_CURRENT012_FULL_BYTE_EXACT_AND_RECOUNT_T44_ACTIVE_NOT_PASS',totals=totals,inverse=inverse['inventory'],xz=xz,
                    source=source,effects=facts,pairs=pairs,preserved_prior_proofs=len(before['prior']),helpers=helpers,
                    scope='Focused custom caret via existing terminal-color owner; complete frames/cursors/clocks unnormalized. DCP tables unread; generation1 forced.'))
else:
    raise SystemExit('before or after; existing reports immutable')
