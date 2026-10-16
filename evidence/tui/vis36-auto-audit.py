"""Independent preserved failures/current CLI-auto inverse and full-frame recount."""
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
BASE = '601006d0911645477b9da583ab772d7a201deb7a'
ROOTS = {}
for attempt, profile in (('001','cli-wide'),('002','cli-wide-r2'),('003','cli-wide-r3')):
    role = 'current' if attempt=='003' else 'failed-'+attempt
    ROOTS[role] = TMP/('vis36-mode-current-'+attempt)/profile
    ROOTS[role+'-logs'] = TMP/('vis36-mode-current-'+attempt)
    ROOTS[role+'-driver'] = TMP/('vis36-mode-capture-r'+str(int(attempt))+'.py')
v.ROOTS = check.ROOTS = ROOTS
check.HEAD = v.frames.HEAD = BASE
tree = ast.parse(check.read_bytes(OUT/'vis37-cancel-intermediate-audit.py'))
functions = [node for node in tree.body if isinstance(node,ast.FunctionDef) and node.name=='save']
check.require(len(functions)==1,'one sealed report writer, no old entrypoint or producer import')
exec(compile(ast.Module(body=functions,type_ignores=[]),'sealed_report_writer','exec'),globals())
BEFORE, AFTER = OUT/'vis36-auto-before.json', OUT/'vis36-auto-after.json'

def source_and_elf():
    source = v.source()
    prior = check.load(TMP/'vis37-form-current-012/wide/source-manifest.json')
    changed = sorted(name for name in source if prior.get(name)!=source[name])
    expected = ['crates/oc-adapters/src/application.rs','crates/oc-core/src/core_app.rs',
                'crates/oc/src/approval_tests.rs','crates/oc/src/cli.rs','crates/oc/src/cli/tests.rs',
                'crates/oc/src/tui_cmd.rs','crates/oc/tests/approval_binary.rs','crates/oc/tests/pty_t39/question.rs',
                'scripts/tui_capture/apply_patch_fixture.py','scripts/tui_capture/check_permission_evidence.mjs',
                'scripts/tui_capture/permission.mjs','scripts/tui_capture/permission_fixture.py']
    check.require(len(source)==713 and set(source)==set(prior) and changed==expected,'only admitted owner/alias/fixture paths changed')
    ep = ROOTS['current']
    lock,manifest,commands = (check.load(ep/name) for name in ('capture.lock.json','source-manifest.json','commands.json'))
    check.require(manifest==source and check.producer_sha(source)==lock['oc']['source_manifest_sha256'] and lock['oc']['commit']==BASE,
                  'exact current713 producer inputs and recorded identity')
    builds = [item for item in commands if item['argv'][0]=='cargo']
    check.require(len(builds)==1 and builds[0]['argv']==['cargo','build','--locked'] and builds[0]['exit_code']==0,'one captured repository build')
    argv = commands[0]['argv']
    pairs = list(zip(argv[2::2],argv[3::2],strict=True))
    check.require(argv[1]==str(REPO/'scripts/tui_capture/capture.mjs') and len(pairs)==len(dict(pairs)) and dict(pairs)=={
        '--reference':str(TMP/'t44-reference/package/bin/opencode'),'--oc':str(REPO/'target/debug/oc'),
        '--build-oc':'true','--geometry':'true','--sample':'short','--sidebar':'hide',
        '--permission':'true','--permission-mode':'auto-cli','--columns':'121','--rows':'40','--output':str(ep)},
        'unaltered actual invocation, no crop/mask/tolerance or alternate profile')
    elf = {side:v.frames.elf(Path(lock[side]['executable_path'])) for side in ('oc','upstream')}
    old = check.load(TMP/'vis37-form-current-012/wide/capture.lock.json')
    check.require(all(elf[s]['sha256']==lock[s]['executable_sha256'] and
                      ((elf[s]['sha256']==old[s]['executable_sha256'])==(s=='upstream')) for s in elf),
                  'rebuilt native and unchanged pinned reference ELF')
    return dict(source_manifest_sha256=check.producer_sha(source),changed_inputs=changed,elf=elf)

def effects(side):
    ep = ROOTS['current']
    records,protocol,inputs = (check.load(ep/side/name) for name in ('permission-checks.json','protocol.json','inputs.json'))
    rows = records['checks']
    check.require(records['status']=='PASS' and records['origin']==side and len(rows)==38,'actual complete lifecycle records')
    named = {row['stage']:row for row in rows}
    check.require(len(named)==37 and collections.Counter(row['stage'] for row in rows)['natural-exit-proof']==2 and named['cli-priority-proof']=={
        'stage':'cli-priority-proof','saved_prompt':True,'root_auto_after_prompt':True,
        'child_once_without_grant':True,'restarted_cli_override':True},'exact named facts and separate proofs for both natural exits')
    snapshots = [row['snapshot'] for row in rows if 'snapshot' in row]
    check.require(len(snapshots)==11 and snapshots==[p for p in protocol if p['kind']=='permission_snapshot'],
                  'actual protocol-bound read-only snapshots')
    check.require(named['settings-auto-config']['snapshot']['cli']['session']['permissions']=='autoaccept' and
                  all(named[n]['snapshot']['cli']['session']['permissions']=='prompt' for n in ('settings-prompt-config','cli-restart-config')),
                  'saved preference and effective CLI are distinct through restart')
    for snap in snapshots:
        for observation in snap['observations']:
            data = observation['data']
            check.require(not data.get('permission_grants',[]) and not data.get('permission',[]),'zero durable Always grants')
    for case in ('autoonce','promptagain','childroot','clirestart'):
        proof = named['auto-'+case+'-proof']
        check.require(proof['files_unchanged'] and proof['no_saved_grants'] and len(proof['actual_results'])==1,
                      'one actual result with no file mutation or saved grant')
    check.require(named['auto-mixed-proof']['files_unchanged'],'mixed Deny unchanged files')
    providers = [p for p in protocol if p['kind']=='provider']
    mains = [p for p in providers if p['operation']=='patch']
    calls = [p for p in protocol if p['kind']=='fixture_tool_call']
    check.require(len(providers)==18 and len(mains)==16 and all(p['valid'] and p['request']['model']=='gpt-fixture-model-1' for p in providers) and
                  collections.Counter(p['case'] for p in mains)=={n:2 for n in ('once','shell','mixed','autoonce','promptagain','childroot','childread','clirestart')} and
                  len(calls)==8 and len([p for p in protocol if p['kind']=='provider_completed'])==18,'exact actual requests/results without replay')
    exits = [p for p in protocol if p['kind']=='exit']
    check.require(len(exits)==2 and all(p['code']==0 and p['termination']=='natural' for p in exits),'two actual natural exits, not harness teardown')
    check.require(sum(p['kind']=='vis36_submit' for p in inputs)==7 and
                  sum(p['kind']=='vis36_settings_toggle' for p in inputs)==2 and
                  [(p['kind'],p['base64']) for p in inputs if p['kind']=='vis36_final_natural_exit']==[
                      ('vis36_final_natural_exit','Aw=='),('vis36_final_natural_exit','Aw==')],
                  'real explicit settings/root submissions and both shutdown inputs')
    data = snapshots[-1]['observations'][0]['data']
    if side=='oc':
        ops = data['tool_operations']
        check.require(len(ops)==8 and collections.Counter(p['state'] for p in ops)=={'completed':7,'failed':1} and
                      len(data['turns'])==8 and all(t['status']=='completed' for t in data['turns']),
                      'typed native terminal states and operation count')
        by_call = {operation['provider_call_id']:operation for operation in ops}
        check.require(len(by_call)==8 and by_call['call_vis36_childroot']['name']=='subagent' and
                      by_call['call_vis36_childread']['name']=='read' and
                      by_call['call_vis36_childroot']['session_id']!=by_call['call_vis36_childread']['session_id'] and
                      by_call['call_vis36_mixed']['state']=='failed' and len(data['patch_effects'])==1,
                      'distinct bound root/child operations and one real admitted patch effect; not a child_jobs query')
        source_ops = dict(operations=len(ops),messages=len(data['messages']),turns=len(data['turns']))
    else:
        messages = data['session_message']
        parts = [(m['id'],i,p) for m in messages if m['type']=='assistant' for i,p in enumerate(json.loads(m['data'])['content']) if p['type']=='tool']
        check.require(len(parts)==8 and collections.Counter(p['state']['status'] for _,_,p in parts)=={'completed':7,'error':1} and
                      collections.Counter(m['type'] for m in messages)=={'user':8,'assistant':16,'idle':8},'actual Original V2 raw content parts and IDs')
        source_ops = dict(operations=len(parts),messages=len(messages))
    return dict(valid_provider_requests=18,main_requests=16,title_requests=2,tool_calls=8,readonly_snapshots=11,natural_exits=2,source=source_ops)

check.require(os.getuid()==1003 and check.git('rev-parse','HEAD').decode().strip()==BASE,'dedicated owner and current base')
helpers = {str(p):check.digest(p) for p in [Path(__file__),HERE/'verifier.py',HERE/'owner_refinement.py',OUT/'vis37-cancel-intermediate-audit.py',*sorted(v.OLD_HELPERS.glob('*.py'))]}
originals,source = v.inventory(),source_and_elf()
if sys.argv[1:]==['facts']:
    print(json.dumps(dict(source=source,effects={s:effects(s) for s in ('oc','upstream')})))
elif sys.argv[1:]==['before']:
    prior = {str(p):dict(sha256=check.digest(p),metadata=check.metadata(p)) for p in sorted(OUT.iterdir())
             if p.name.endswith(('.tar.xz','.inventory.json','.archived.json'))}
    save(BEFORE,dict(status='INDEPENDENT_VIS36_AUTO_SEALED_BEFORE',originals=originals,source=source,helpers=helpers,prior=prior))
elif sys.argv[1:]==['after']:
    before = check.load(BEFORE)
    check.require(originals==before['originals'] and source==before['source'] and helpers==before['helpers'],'all sealed bytes/meta/source unchanged')
    archive,receipt = OUT/'vis36-auto-proof.tar.xz',check.load(OUT/'vis36-auto-proof.inventory.json')
    raw,xz = check.inverse_xz(check.read_bytes(archive))
    restored,inverse = check.inverse_ustar(raw,originals['files'])
    check.require(check.digest(archive)==receipt['archive_sha256'] and check.sha(raw)==receipt['tar_sha256'] and inverse['inventory']==receipt['inventory'],
                  'manual entire single-XZ/canonical-USTAR inverse of all failed/current original bytes')
    del raw,restored
    ep,lock = ROOTS['current'],check.load(ROOTS['current']/'capture.lock.json')
    stages = [item['stage'] for item in check.load(ep/'oc/permission-checks.json')['checks'] if item.get('status')=='CAPTURED_PERMISSION']
    check.require(len(stages)==16,'all complete current whole frames')
    pairs = []
    for stage in stages:
        name = 'permission-'+stage
        grids = {s:check.load(ep/s/(name+'.cells.json')) for s in ('oc','upstream')}
        grid,_ = v.frames.compare_grid(grids['upstream'],grids['oc'])
        png = v.frames.compare_png(ep/'upstream'/(name+'.png'),ep/'oc'/(name+'.png'))
        for mode,metric in (('grid',grid),('png',png)):
            recorded = check.load(ep/(name+'.'+mode+'-diff.json'))
            check.require(all(recorded[k]==metric[k] for k in metric if k!='different_fields'),'independent complete styled-cell/cursor/PNG recount')
            equal = (grid['different_cells']==0 and not grid['cursor_differs']) if mode=='grid' else png['different_pixels']==0
            check.require(recorded['status']==('PASS' if equal else 'FAIL'),'no mask/crop/tolerance or normalization')
        pairs.append(dict(stage=stage,grid=grid,png=png))
    facts = {side:effects(side) for side in ('oc','upstream')}
    check.require(v.inventory()==originals and source_and_elf()==source and all(check.digest(p)==s for p,s in helpers.items()) and
                  all(check.digest(p)==r['sha256'] and check.metadata(p)==r['metadata'] for p,r in before['prior'].items()),'all originals/source/ELFs/helpers/old proof bytes and metadata unchanged')
    totals = dict(paired_frames=len(pairs),whole_pngs=2*len(pairs),checked_cells=sum(p['grid']['cells_checked'] for p in pairs),
                  checked_pixels=sum(p['png']['pixels_checked'] for p in pairs),different_cells=sum(p['grid']['different_cells'] for p in pairs),
                  different_pixels=sum(p['png']['different_pixels'] for p in pairs),equal_cursor_pairs=sum(not p['grid']['cursor_differs'] for p in pairs))
    save(AFTER,dict(status='INDEPENDENT_VIS36_AUTO_FULL_BYTE_EXACT_AND_RECOUNT_T44_ACTIVE_NOT_PASS',source=source,helpers=helpers,totals=totals,
                    inverse=inverse['inventory'],xz=xz,pairs=pairs,effects=facts,preserved_prior_proofs=len(before['prior']),
                    scope='Real CLI-auto root/child/settings/restart and two natural exits. SQLite is read-only, not CoreEvent/syscall proof. All failed episodes retained; no pixel PASS.'))
else:
    raise SystemExit('before or after; existing reports immutable')
