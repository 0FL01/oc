"""Independent current009 complete inverse/recount; question-only tab padding roles."""
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
BASE = 'a0655da267ae07328944447d3160c8f901a59b3a'
v.ROOTS = {**{'current-'+p:TMP/'vis37-form-current-009'/p for p in ('small','wide','tall')},
           'current-logs':TMP/'vis37-form-current-009','current-driver':TMP/'vis37-form-capture-r9.py'}
check.ROOTS = v.ROOTS
check.HEAD = v.frames.HEAD = BASE
STAGES = list(check.STAGES)
STAGES.insert(3,'question-multiple-focused')
READER = OUT/'vis37-focus-paint-audit.py'
INTERMEDIATE = OUT/'vis37-cancel-intermediate-audit.py'
for path,names in ((READER,('strict_commands','effects')),(INTERMEDIATE,('save','cancel_row'))):
    tree = ast.parse(check.read_bytes(path), filename=str(path))
    functions = [n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in names]
    check.require(len(functions)==len(names),'exact delivered independent functions, no producer codec or old entrypoint')
    exec(compile(ast.Module(body=functions,type_ignores=[]),str(path),'exec'),globals())
BEFORE,AFTER = OUT/'vis37-tab-padding-before.json',OUT/'vis37-tab-padding-after.json'

def source_and_elf():
    source = v.source()
    prior = check.load(TMP/'vis37-form-current-008/wide/source-manifest.json')
    changed = sorted(n for n in source if prior.get(n)!=source[n])
    check.require(len(source)==713 and set(source)==set(prior) and changed==[
        'crates/oc-tui/src/shell.rs','crates/oc-tui/src/shell/tests.rs'],
        'only shell question-attention padding and nearest regression changed')
    elf = {}
    for profile in ('small','wide','tall'):
        ep = v.ROOTS['current-'+profile]
        lock,manifest,commands = (check.load(ep/n) for n in ('capture.lock.json','source-manifest.json','commands.json'))
        check.require(manifest==source and check.producer_sha(source)==lock['oc']['source_manifest_sha256'] and lock['oc']['commit']==BASE,
                      'actual713 source and unchanged recorded producer identity')
        builds = [c for c in commands if c['argv'][0]=='cargo']
        check.require(len(builds)==1 and builds[0]['argv']==['cargo','build','--locked'] and builds[0]['exit_code']==0,'one exact acquisition build')
        strict_commands(ep,lock,commands)
        old = check.load(TMP/'vis37-form-current-008'/profile/'capture.lock.json')
        for side in ('oc','upstream'):
            actual = v.frames.elf(Path(lock[side]['executable_path']))
            check.require(actual['sha256']==lock[side]['executable_sha256'] and
                          ((actual['sha256']==old[side]['executable_sha256'])==(side=='upstream')),'rebuilt native and unchanged pinned donor')
            elf[side] = actual
    return dict(source_manifest_sha256=check.producer_sha(source),changed_inputs=changed,elf=elf)

def header_padding(grids):
    # Named role diagnostic only. Every whole frame/cursor/pixel is independently
    # compared below, including the separate pulse background differences.
    title = 'VIS question fixture'
    layout,question = {},None
    for side,grid in grids.items():
        row = grid['cells'][0]
        text = ''.join(c['symbol'] for c in row)
        check.require(text.count(title)==1,'actual named header title')
        left = text.index(title)
        check.require(left==3,'observed standard indicator/title geometry')
        active = row[1]['symbol']=='?'
        check.require(question is None or question==active,'same real question attention')
        question = active
        indexes = [0,2,*range(left+len(title),32)]
        check.require(len(indexes)==11 and all(row[x]['symbol']==' ' and row[x]['modifiers']==[] for x in indexes),'11 actual layout blanks, not internal title spaces')
        layout[side] = {str(x):row[x] for x in indexes}
    check.require(all(layout['oc'][x]['fg']==layout['upstream'][x]['fg'] for x in layout['oc']),
                  'all11 layout foregrounds equal in question and non-question states')
    return dict(question_attention=question,layout_cells=layout,all_named_foregrounds_equal=True,
                background_differences=[x for x in layout['oc'] if layout['oc'][x]['bg']!=layout['upstream'][x]['bg']],
                diagnostic_only_full_frames_and_cursor_remain_strict=True)

check.require(os.getuid()==1003 and check.git('rev-parse','HEAD').decode().strip()==BASE,'dedicated owner and actual base')
helpers = {str(p):check.digest(p) for p in [Path(__file__),READER,INTERMEDIATE,HERE/'verifier.py',HERE/'owner_refinement.py',*sorted(v.OLD_HELPERS.glob('*.py'))]}
originals,source = v.inventory(),source_and_elf()
if sys.argv[1:]==['before']:
    prior = {str(p):dict(sha256=check.digest(p),metadata=check.metadata(p)) for p in sorted(OUT.iterdir())
             if p.name.endswith(('.tar.xz','.inventory.json','.archived.json'))}
    save(BEFORE,dict(status='INDEPENDENT_CURRENT009_SEALED_BEFORE',originals=originals,source=source,helpers=helpers,prior=prior))
elif sys.argv[1:]==['after']:
    before = check.load(BEFORE)
    check.require(originals==before['originals'] and source==before['source'] and helpers==before['helpers'],'all before identities retained')
    archive,receipt_path = OUT/'vis37-form-r9-proof.tar.xz',OUT/'vis37-form-r9-proof.inventory.json'
    receipt = check.load(receipt_path)
    raw,xz = check.inverse_xz(check.read_bytes(archive))
    restored,inverse = check.inverse_ustar(raw,originals['files'])
    check.require(check.digest(archive)==receipt['archive_sha256'] and check.sha(raw)==receipt['tar_sha256'] and inverse['inventory']==receipt['inventory'],
                  'manual canonical XZ/USTAR every original whole byte and direct alias')
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
            if stage!='question-home': pair['header_padding'] = header_padding(grids)
            if stage in STAGES[1:9]+['question-cancel-pending']: pair['pending_owner'] = v.owner(grids,'Questions')
            if stage=='question-answered-card': pair['answered_owner'] = v.owner(grids,'# Questions')
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
                  question_header_foreground_equal_pairs=sum(p.get('header_padding',{}).get('question_attention',False) for p in pairs),
                  non_question_header_foreground_equal_pairs=sum(not p['header_padding']['question_attention'] for p in pairs if 'header_padding' in p))
    check.require(v.inventory()==originals and source_and_elf()==source and all(check.digest(p)==sha for p,sha in helpers.items()) and
                  all(check.digest(p)==r['sha256'] and check.metadata(p)==r['metadata'] for p,r in before['prior'].items()),'all original metadata/source/ELFs/helpers/prior proofs unchanged')
    save(AFTER,dict(status='INDEPENDENT_CURRENT009_FULL_BYTE_EXACT_AND_RECOUNT_T44_ACTIVE_NOT_PASS',totals=totals,inverse=inverse['inventory'],xz=xz,
                    source=source,effects=facts,pairs=pairs,preserved_prior_proofs=len(before['prior']),helpers=helpers,
                    scope='Question-attention header layout foreground only; non-question/canvas foreground preserved. Whole pixels/cursor and pulse background are not normalized. DCP tables unread; generation1 forced.'))
else:
    raise SystemExit('before or after; existing reports immutable')
