"""Independent current011 complete inverse/recount; real question status geometry."""
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
BASE = 'd4cfa8698b67e57036a0275a94f541b8f1b6c6d2'
v.ROOTS = {**{'current-'+p:TMP/'vis37-form-current-011'/p for p in ('small','wide','tall')},
           'current-logs':TMP/'vis37-form-current-011','current-driver':TMP/'vis37-form-capture-r11.py'}
check.ROOTS = v.ROOTS
check.HEAD = v.frames.HEAD = BASE
STAGES = list(check.STAGES)
STAGES.insert(3,'question-multiple-focused')
READER = OUT/'vis37-focus-paint-audit.py'
INTERMEDIATE = OUT/'vis37-cancel-intermediate-audit.py'
PADDING = OUT/'vis37-tab-padding-audit.py'
COLORS = OUT/'vis37-tab-color-final-audit.py'
for path,names in ((READER,('strict_commands','effects')),(INTERMEDIATE,('save','cancel_row')),
                   (PADDING,('source_and_elf','header_padding')),(COLORS,('full_header',))):
    tree = ast.parse(check.read_bytes(path), filename=str(path))
    functions = [n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in names]
    check.require(len(functions)==len(names),'exact delivered independent functions; no producer codec or old entrypoint')
    if path==PADDING:
        replacements = {'vis37-form-current-008/wide/source-manifest.json':'vis37-form-current-010/wide/source-manifest.json',
                        'vis37-form-current-008':'vis37-form-current-010'}
        older = [n for f in functions if f.name=='source_and_elf' for n in ast.walk(f)
                 if isinstance(n,ast.Constant) and n.value in replacements]
        check.require(len(older)==2 and {n.value for n in older}==set(replacements),'two explicit source/build comparison paths')
        for node in older: node.value=replacements[node.value]
    exec(compile(ast.Module(body=functions,type_ignores=[]),str(path),'exec'),globals())
BEFORE,AFTER = OUT/'vis37-question-status-before.json',OUT/'vis37-question-status-after.json'

def small_underlay(grids):
    # Additional named diagnostic, never a crop/mask of the full comparison.
    # The first question belongs to one live request, so there is no completed
    # assistant duration to pretend equal or normalize in these three states.
    regions = {}
    for side,grid in grids.items():
        rows = [''.join(c['symbol'] for c in row) for row in grid['cells']]
        hits = [y for y,row in enumerate(rows) if row.strip()=='┃  Questions']
        check.require(len(hits)==1 and hits[0]>2,'one actual pending form heading')
        top = hits[0]-1
        check.require(not rows[top-1].strip(),'real blank status sibling before form')
        regions[side] = dict(end=top,cells=grid['cells'][1:top])
    check.require(regions['oc']==regions['upstream'],'complete real small transcript/status rows match every styled cell')
    return dict(start=1,end=regions['oc']['end'],all_underlay_styled_cells_equal=True)

check.require(os.getuid()==1003 and check.git('rev-parse','HEAD').decode().strip()==BASE,'dedicated owner and actual base')
helpers = {str(p):check.digest(p) for p in [Path(__file__),READER,INTERMEDIATE,PADDING,COLORS,HERE/'verifier.py',HERE/'owner_refinement.py',*sorted(v.OLD_HELPERS.glob('*.py'))]}
originals,source = v.inventory(),source_and_elf()
if sys.argv[1:]==['before']:
    prior = {str(p):dict(sha256=check.digest(p),metadata=check.metadata(p)) for p in sorted(OUT.iterdir())
             if p.name.endswith(('.tar.xz','.inventory.json','.archived.json'))}
    save(BEFORE,dict(status='INDEPENDENT_CURRENT011_SEALED_BEFORE',originals=originals,source=source,helpers=helpers,prior=prior))
elif sys.argv[1:]==['after']:
    before = check.load(BEFORE)
    check.require(originals==before['originals'] and source==before['source'] and helpers==before['helpers'],'all before identities retained')
    archive,receipt_path = OUT/'vis37-form-r11-proof.tar.xz',OUT/'vis37-form-r11-proof.inventory.json'
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
                  small_underlay_equal_pairs=sum(p.get('small_underlay',{}).get('all_underlay_styled_cells_equal',False) for p in pairs))
    check.require(v.inventory()==originals and source_and_elf()==source and all(check.digest(p)==sha for p,sha in helpers.items()) and
                  all(check.digest(p)==r['sha256'] and check.metadata(p)==r['metadata'] for p,r in before['prior'].items()),'all original metadata/source/ELFs/helpers/prior proofs unchanged')
    save(AFTER,dict(status='INDEPENDENT_CURRENT011_FULL_BYTE_EXACT_AND_RECOUNT_T44_ACTIVE_NOT_PASS',totals=totals,inverse=inverse['inventory'],xz=xz,
                    source=source,effects=facts,pairs=pairs,preserved_prior_proofs=len(before['prior']),helpers=helpers,
                    scope='Real status sibling and sticky slice; complete original frames/cursors/clocks unnormalized. DCP tables unread; generation1 forced.'))
else:
    raise SystemExit('before or after; existing reports immutable')
