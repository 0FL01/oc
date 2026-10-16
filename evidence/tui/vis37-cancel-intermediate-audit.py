"""Independent current006 inverse/recount; known question icon-gap residual."""
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
BASE = '7a7ccf564ae7c593327070fcc51edbd32e95f5f7'
v.ROOTS = {**{'current-'+p: TMP/'vis37-form-current-006'/p for p in ('small','wide','tall')},
           'current-logs': TMP/'vis37-form-current-006', 'current-driver': TMP/'vis37-form-capture-r6.py'}
check.ROOTS = v.ROOTS
check.HEAD = v.frames.HEAD = BASE
STAGES = list(check.STAGES)
STAGES.insert(3, 'question-multiple-focused')
READER = OUT/'vis37-focus-paint-audit.py'
# Only the previously delivered observer functions; no old executable entrypoint
# or producer encoder/audit is imported, and its file remains immutable.
tree = ast.parse(check.read_bytes(READER), filename=str(READER))
functions = [n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in ('strict_commands','effects')]
check.require(len(functions)==2, 'two delivered independent observer functions')
exec(compile(ast.Module(body=functions,type_ignores=[]), str(READER),'exec'), globals())
BEFORE, AFTER = OUT/'vis37-cancel-intermediate-before.json', OUT/'vis37-cancel-intermediate-after.json'

def save(path, value):
    data=(json.dumps(value,ensure_ascii=False,separators=(',',':'))+'\n').encode()
    check.require(len(data)<=check.FILE and check.quota()['headroom_bytes'] >= (len(data)+4095)//4096*4096, 'bounded exclusive report')
    with path.open('xb') as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    print(json.dumps(dict(output=str(path),bytes=len(data),sha256=check.sha(data),status=value['status'],
                         totals=value.get('totals'),memory=check.proc_memory(),quota=check.quota())))

def source_and_elf():
    source=v.source()
    prior=check.load(TMP/'vis37-form-current-005/wide/source-manifest.json')
    changed=sorted(n for n in source if prior.get(n)!=source[n])
    check.require(len(source)==713 and set(source)==set(prior) and changed==[
        'crates/oc-tui/src/tools.rs','crates/oc-tui/src/tools/presentation_tests.rs','crates/oc/tests/pty_t39/question.rs'],
        'only cancellation renderer and its nearest unit/actual consumer inputs changed')
    elf={}
    for profile in ('small','wide','tall'):
        ep=v.ROOTS['current-'+profile]
        lock,manifest,commands=(check.load(ep/n) for n in ('capture.lock.json','source-manifest.json','commands.json'))
        check.require(manifest==source and check.producer_sha(source)==lock['oc']['source_manifest_sha256'] and lock['oc']['commit']==BASE,
                      'complete actual current source and original producer binding')
        builds=[c for c in commands if c['argv'][0]=='cargo']
        check.require(len(builds)==1 and builds[0]['argv']==['cargo','build','--locked'] and builds[0]['exit_code']==0,'one exact acquisition build')
        strict_commands(ep,lock,commands)
        old=check.load(TMP/'vis37-form-current-005'/profile/'capture.lock.json')
        for side in ('oc','upstream'):
            actual=v.frames.elf(Path(lock[side]['executable_path']))
            check.require(actual['sha256']==lock[side]['executable_sha256'] and
                          ((actual['sha256']==old[side]['executable_sha256']) == (side=='upstream')), 'rebuilt native and unchanged pinned donor')
            elf[side]=actual
    return dict(source_manifest_sha256=check.producer_sha(source),changed_inputs=changed,elf=elf)

def cancel_row(grids):
    found={}
    for side,grid in grids.items():
        rows=[''.join(c['symbol'] for c in row) for row in grid['cells']]
        target='→ Asked 2 questions'
        hits=[(y,row.index(target)) for y,row in enumerate(rows) if target in row]
        check.require(len(hits)==1 and not any('error: question dismissed' in row or 'Questions dismissed' in row for row in rows),
                      'one real TUI cancelled label, no raw error or web-only status row')
        y,x=hits[0]
        found[side]=dict(x=x,y=y,cells=grid['cells'][y][x:x+len(target)])
    differences=[dict(offset=i,reference=a,actual=b) for i,(a,b) in enumerate(zip(found['upstream']['cells'],found['oc']['cells'],strict=True)) if a!=b]
    return dict(geometry={s:[r['x'],r['y']] for s,r in found.items()},differences=differences,
                all_named_cells_equal=not differences,diagnostic_only_full_frames_remain_strictly_compared=True)

check.require(os.getuid()==1003 and check.git('rev-parse','HEAD').decode().strip()==BASE,'dedicated owner and actual base')
helpers={str(p):check.digest(p) for p in [Path(__file__),READER,HERE/'verifier.py',HERE/'owner_refinement.py',*sorted(v.OLD_HELPERS.glob('*.py'))]}
originals,source=v.inventory(),source_and_elf()
if sys.argv[1:]==['before']:
    prior={str(p):dict(sha256=check.digest(p),metadata=check.metadata(p)) for p in sorted(OUT.iterdir())
           if p.name.endswith(('.tar.xz','.inventory.json','.archived.json'))}
    save(BEFORE,dict(status='INDEPENDENT_CURRENT006_INTERMEDIATE_SEALED_BEFORE',originals=originals,source=source,helpers=helpers,prior=prior))
elif sys.argv[1:]==['after']:
    before=check.load(BEFORE)
    check.require(originals==before['originals'] and source==before['source'] and helpers==before['helpers'],'before identity retained')
    archive,receipt_path=OUT/'vis37-form-r6-proof.tar.xz',OUT/'vis37-form-r6-proof.inventory.json'
    receipt=check.load(receipt_path)
    raw,xz=check.inverse_xz(check.read_bytes(archive))
    restored,inverse=check.inverse_ustar(raw,originals['files'])
    check.require(check.digest(archive)==receipt['archive_sha256'] and check.sha(raw)==receipt['tar_sha256'] and inverse['inventory']==receipt['inventory'],
                  'every original whole byte and canonical manual inverse')
    del restored,raw
    pairs,facts=[],{}
    for profile in ('small','wide','tall'):
        ep=v.ROOTS['current-'+profile]
        facts.update({profile+'-'+s:effects(ep,s) for s in ('oc','upstream')})
        for stage in STAGES:
            name='tool-preview-'+stage
            grids={s:check.load(ep/s/(name+'.cells.json')) for s in ('oc','upstream')}
            metric,_=v.frames.compare_grid(grids['upstream'],grids['oc'])
            png=v.frames.compare_png(ep/'upstream'/(name+'.png'),ep/'oc'/(name+'.png'))
            for mode,value in (('grid',metric),('png',png)):
                recorded=check.load(ep/(name+'.'+mode+'-diff.json'))
                check.require(all(recorded[k]==value[k] for k in value if k!='different_fields'),'whole independent recount')
            pair=dict(profile=profile,stage=stage,grid=metric,png=png)
            if stage in STAGES[1:9]+['question-cancel-pending']: pair['pending_owner']=v.owner(grids,'Questions')
            if stage=='question-answered-card': pair['answered_owner']=v.owner(grids,'# Questions')
            if stage in ('question-dismissed','question-cancel-reopened'): pair['cancelled_row']=cancel_row(grids)
            pairs.append(pair)
    totals=dict(paired_frames=len(pairs),whole_pngs=len(pairs)*2,checked_cells=sum(p['grid']['cells_checked'] for p in pairs),
                checked_pixels=sum(p['png']['pixels_checked'] for p in pairs),different_cells=sum(p['grid']['different_cells'] for p in pairs),
                different_pixels=sum(p['png']['different_pixels'] for p in pairs),equal_cursor_pairs=sum(not p['grid']['cursor_differs'] for p in pairs),
                pending_owner_equal_pairs=sum(p.get('pending_owner',{}).get('all_owner_styled_cells_equal',False) for p in pairs),
                answered_owner_equal_pairs=sum(p.get('answered_owner',{}).get('all_owner_styled_cells_equal',False) for p in pairs))
    check.require(v.inventory()==originals and source_and_elf()==source and all(check.digest(p)==sha for p,sha in helpers.items()) and
                  all(check.digest(p)==r['sha256'] and check.metadata(p)==r['metadata'] for p,r in before['prior'].items()),'all original metadata/source/ELFs/helpers/prior proofs unchanged')
    save(AFTER,dict(status='INDEPENDENT_CURRENT006_INTERMEDIATE_FULL_BYTE_EXACT_T44_NOT_PASS',totals=totals,inverse=inverse['inventory'],xz=xz,
                    source=source,effects=facts,pairs=pairs,preserved_prior_proofs=len(before['prior']),helpers=helpers,
                    scope='All full comparisons DIFFERENT. Named cancellation presentation has known icon-layout blank FG/strike residual; fix it without discarding006. DCP tables unread; final generation1 forced.'))
else:
    raise SystemExit('before or after; existing reports immutable')
