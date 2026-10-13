"""Independent read-only seals/codec/frame audit for the Generic summary slice."""
import ast
import gc
import importlib.util
import json
import os
from pathlib import Path
import resource
import re
import stat
import sys
import types

sys.dont_write_bytecode = True
resource.setrlimit(resource.RLIMIT_AS, (384 * 1024**2,) * 2)
REPO = Path('/home/opencode/ai/oc')
TMP = Path('/home/opencode/.cache/opencode-tmp/opencode')
OUT = REPO / 'evidence/tui'
BASE = 'f2f2025e060bba9ab2d41932b888d344a88ee88b'
BEFORE = TMP / 'vis36-generic-summary-before.json'

def declarations(path, functions, assignments=(), imports=False):
    tree = ast.parse(path.read_bytes(), filename=str(path))
    nodes = [n for n in tree.body if
             (imports and isinstance(n, (ast.Import, ast.ImportFrom))) or
             (isinstance(n, ast.FunctionDef) and n.name in functions) or
             (isinstance(n, ast.Assign) and any(isinstance(t, ast.Name) and t.id in assignments for t in n.targets))]
    assert {n.name for n in nodes if isinstance(n, ast.FunctionDef)} == set(functions)
    return ast.fix_missing_locations(ast.Module(body=nodes, type_ignores=[]))

check = types.ModuleType('independent_generic_summary')
source = TMP / 'vis37-independent-20261012-sole-verifier/inverse_verifier.py'
names = {'require','sha','digest','read_bytes','load','canonical_sha','metadata','git','producer_sha',
         'proc_memory','quota','vli','inverse_xz','cstring','octal','make_header','path_ok','inverse_ustar','compare_grid','decode_png','compare_png','elf'}
exec(compile(declarations(source, names, ('SCOPE','PREFIXES'), True), str(source), 'exec'), check.__dict__)
check.AS, check.FILE, check.TOTAL, check.OWNED = 384*1024**2, 16*1024**2, 128*1024**2, 1024**3
check.MEMBERS, check.XZ_DICT, check.XZ_DECODE = 2000, 16*1024**2, 32*1024**2
check.REPO, check.TEMP = REPO, TMP
check.PREFIXES = (*check.PREFIXES, 'vis36-')
check.ROOTS = {}
check.DONOR = '2670273ff17da96f85c5826ced57aa1b368754fa'
check.HEAD = BASE
check.FIELDS = {'symbol','fg','bg','modifiers','width'}
sys.path.insert(0,str(REPO/'scripts'))

def module(name,path):
    spec=importlib.util.spec_from_file_location(name,path)
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value)
    return value

producer=module('generic_summary_strict_encoder',REPO/'scripts/retain_syntax_proof.py')
effects_ns=dict(json=json,collections=check.collections)
exec(compile(declarations(OUT/'vis36-prompt-audit.py',{'effects'},('STAGES',)), 'prompt_effects_declarations', 'exec'),effects_ns)
STAGES=effects_ns['STAGES']
PROFILES=('wide','small')

def current_source():
    names = check.git('ls-files','-z','--cached','--others','--exclude-standard','--',*check.SCOPE).decode().split('\0')
    return {n:check.digest(REPO/n) for n in sorted(set(names)) if n and (REPO/n).exists()}

def record(path):
    return dict(sha256=check.digest(path), metadata=check.metadata(path))

def save(path, value):
    data = (json.dumps(value, indent=2, ensure_ascii=False)+'\n').encode()
    assert len(data) <= check.FILE
    with path.open('xb') as f:
        f.write(data); f.flush(); os.fsync(f.fileno())

def preservation(before):
    for group in ('prior','helpers'):
        for path,row in before[group].items():
            assert record(path) == row, path
    for path,row in before['original_files'].items():
        assert check.digest(path) == row[0] and check.metadata(path) == row[1], path
    for path,meta in before['original_dirs'].items():
        assert check.metadata(path) == meta, path

def baseline():
    assert os.getuid() == 1003 and check.git('rev-parse','HEAD').decode().strip() == BASE
    assert not check.git('diff','--name-only') and not check.git('diff','--cached','--name-only')
    old = check.load(TMP/'vis38-durable-baseline-20261013.json')
    preservation(old)
    files, dirs, helpers = dict(old['original_files']), dict(old['original_dirs']), dict(old['helpers'])
    for receipt in sorted(OUT.glob('vis36-vis38-durable-*.inventory.json')):
        inv = check.load(receipt)
        for row in inv['original_members'].values():
            files[row['path']] = [row['sha256'],row['original_metadata']]
        dirs.update(inv['original_directories'])
    for path in sorted(TMP.glob('vis36-*.py'))+sorted(TMP.glob('vis37-*.py'))+sorted(OUT.glob('vis*.py')):
        if path != Path(__file__): helpers[str(path)] = record(path)
    prior = {str(p):record(p) for p in sorted(OUT.iterdir()) if p.name.endswith(('.tar.xz','.inventory.json','.archived.json'))}
    sources = current_source()
    assert check.producer_sha(sources) == 'ced0ae5ba472051f8a61e7e0378249cc4d81194368b6088fc79c061af9ffbf6a'
    value = dict(source=sources, source_sha256=check.producer_sha(sources),
        ELF={str(p):check.digest(p) for p in (REPO/'target/debug/oc',REPO/'target/release/oc',TMP/'t44-reference/package/bin/opencode')},
        prior=prior,helpers=helpers,original_files=files,original_dirs=dirs,
        state=check.load(REPO/'progress/STATE.json'),quota=check.quota())
    preservation(value);save(BEFORE,value)
    print(json.dumps(dict(status='BASELINE_SEALED', prior=len(prior),files=len(files),directories=len(dirs),quota=check.quota())))

def roots(profile):
    return producer.vis36_prompt_roots(profile,'007')

def inventory(mapping,files):
    expected={}
    for name,data in files.items():
        role,relative=name.split('/',1)
        root=mapping[role]
        p=root if role.endswith('-driver') else root/relative
        assert check.read_bytes(p)==data
        expected[name]=dict(path=str(p),bytes=len(data),sha256=check.sha(data),original_metadata=check.metadata(p))
    dirs={}
    for role,root in mapping.items():
        if role in ('gates','programs'):continue
        paths=[root,*sorted(root.rglob('*'))] if root.is_dir() and not role.endswith('-logs') else [root]
        for p in paths:
            if p.is_dir():dirs[str(p)]=check.metadata(p)
    return dict(original_members=expected,original_directories=dirs,source_roots={k:str(v) for k,v in mapping.items()})

def acquisition():
    before=check.load(BEFORE);preservation(before)
    current=current_source()
    changed=sorted(k for k in current if current[k]!=before['source'].get(k))
    assert set(current)==set(before['source']) and changed==['crates/oc-tui/src/tools.rs','crates/oc-tui/src/tools/presentation_tests.rs']
    value=dict(source=current,source_sha256=check.producer_sha(current),source_count=len(current),changed_inputs=changed,
        ELF={str(p):check.elf(p) for p in (REPO/'target/debug/oc',REPO/'target/release/oc',TMP/'t44-reference/package/bin/opencode')},profiles={})
    for profile in PROFILES:
        mapping=roots(profile);producer.ROOTS=mapping
        files=producer.source_files()
        strict=producer.vis36_prompt_checks(files,profile,'007')
        value['profiles'][profile]={**inventory(mapping,files),'strict_producer_checks':strict,
            'effects':{s:effects_ns['effects'](mapping['current'],s) for s in ('oc','upstream')}}
        del files;gc.collect()
    value['quota']=check.quota();save(TMP/'vis36-generic-summary-acquisition-007-before.json',value)
    print(json.dumps(dict(status='ACQUISITION_SEALED',source_sha256=value['source_sha256'],source_count=len(current),
        profiles={p:dict(members=len(v['original_members']),logical_bytes=sum(r['bytes'] for r in v['original_members'].values()),directories=len(v['original_directories'])) for p,v in value['profiles'].items()},quota=value['quota'])))

def inverse(archive,inv):
    check.ROOTS={k:Path(v) for k,v in inv['source_roots'].items()}
    encoded=check.read_bytes(archive)
    raw,xz=check.inverse_xz(encoded)
    restored,tar=check.inverse_ustar(raw,inv['original_members'])
    for row in inv['original_members'].values():
        assert record(row['path'])==dict(sha256=row['sha256'],metadata=row['original_metadata']),row['path']
    for path,meta in inv['original_directories'].items():assert check.metadata(path)==meta,path
    result=dict(archive_sha256=check.sha(encoded),archive_bytes=len(encoded),tar_sha256=check.sha(raw),tar_bytes=len(raw),xz=xz,tar=tar)
    del raw,restored,encoded;gc.collect()
    return result

def whole_frames(profile):
    root=roots(profile)['current'];lock=check.load(root/'capture.lock.json');commands=check.load(root/'commands.json')
    source=current_source()
    assert check.load(root/'source-manifest.json')==source and check.producer_sha(source)==lock['oc']['source_manifest_sha256']
    assert lock['oc']['commit']==BASE and lock['sources']['upstream_commit']==check.DONOR
    assert [a['scenario'] for a in lock['attempts'] if a['mode']=='grid']==['permission-'+s for s in STAGES]
    assert len(lock['captures'])==len(lock['attempts'])==102
    assert len([c for c in commands if c['argv'][0]=='cargo'])==1
    frames=[]
    for stage in STAGES:
        name='permission-'+stage
        grids=[check.load(root/s/(name+'.cells.json')) for s in ('upstream','oc')]
        assert grids[0]['fixture_sha256']==lock['fixture_sha256']
        assert grids[0]['environment_id']==check.producer_sha(lock['profile'])
        grid,diffs=check.compare_grid(*grids)
        png=check.compare_png(*(check.read_bytes(root/s/(name+'.png')) for s in ('upstream','oc')))
        for mode,actual,suffix in (('grid',grid,'.cells.json'),('png',png,'.png')):
            report=root/(name+'.'+mode+'-diff.json');recorded=check.load(report)
            assert all(recorded[k]==v for k,v in actual.items() if k!='different_fields')
            equal=actual['different_cells']==0 and not actual['cursor_differs'] if mode=='grid' else actual['different_pixels']==0
            assert recorded['status']==('PASS' if equal else 'FAIL')
            paths=[root/s/(name+suffix) for s in ('upstream','oc')]
            assert [recorded[k] for k in ('reference_sha256','actual_sha256')]==[check.digest(p) for p in paths]
            argv=['/usr/bin/python3',str(REPO/'tui-recovery/scripts/compare_frames.py'),mode,*map(str,paths),'--report',str(report)]
            found=[c for c in commands if c['argv']==argv]
            attempts=[a for a in lock['attempts'] if a['scenario']==name and a['mode']==mode]
            assert len(found)==len(attempts)==1
            assert found[0]['exit_code']==attempts[0]['exit_code']==(0 if equal else 1)
            assert attempts[0]['status']==('EQUAL' if equal else 'DIFFERENT') and json.loads(found[0]['stdout'])==recorded
        frames.append(dict(stage=stage,grid=grid,png=png))
    grids=[check.load(root/s/'permission-mcp-completed.cells.json') for s in ('upstream','oc')]
    rows=[]
    for y,(a,b) in enumerate(zip(grids[0]['cells'],grids[1]['cells'],strict=True)):
        text=''.join(c['symbol'] for c in a)
        if 'words wrap' in text:
            rows.append(dict(y=y,original=text,native=''.join(c['symbol'] for c in b),all_fields_equal=a==b,
                differences=[dict(x=x,fields=sorted(k for k in check.FIELDS if ac[k]!=bc[k])) for x,(ac,bc) in enumerate(zip(a,b,strict=True)) if ac!=bc]))
    assert rows
    assert any(r['y']==27 and r['all_fields_equal'] for r in rows), 'complete previously shortened suffix row'
    return dict(frames=frames,summary_rows=rows,pairs=len(frames),cells=sum(f['grid']['cells_checked'] for f in frames),
        pixels=sum(f['png']['pixels_checked'] for f in frames),equal_grids=sum(f['grid']['different_cells']==0 and not f['grid']['cursor_differs'] for f in frames),
        equal_pngs=sum(f['png']['different_pixels']==0 for f in frames),equal_cursors=sum(not f['grid']['cursor_differs'] for f in frames))

def gates():
    records={}
    for path in sorted(TMP.glob('vis37-form-gate-vis36-generic-summary-*.json')):
        row=check.load(path);log=Path(row['log']);data=check.read_bytes(log)
        assert len(data)==row['log_bytes']
        results=re.findall(rb'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',data)
        assert len(results)==row['rust_targets']
        assert [sum(int(r[n]) for r in results) for n in range(3)]==[row[k] for k in ('passed','failed','ignored')]
        records[row['label']]=dict(command=row['command'],exit_code=row['exit_code'],passed=row['passed'],failed=row['failed'],ignored=row['ignored'],
            targets=row['rust_targets'],log_sha256=check.sha(data),record_sha256=check.digest(path),elapsed_ns=row['elapsed_ns'],
            panics=re.findall(r"thread '([^']+)' .*panicked",data.decode(errors='replace')))
    prefix='vis36-generic-summary-'
    for label in ('final-targeted','final-clippy','current-fmt','current-debug-locked','current-debug-ordinary','current-debug-help',
                  'current-release-build','current-release-help','current-python','restored-debug','restored-release',
                  'postworkspace-debug','postworkspace-release','current-crate-serial','current-workspace-serial'):
        assert records[prefix+label]['exit_code']==0,label
    assert records[prefix+'final-targeted']['passed']==11
    release={'startup':2,'term01':1,'panic':1,'vis31':2,'s07':1,'vis38':1,'aud32':1}
    for label,count in release.items():
        row=records[prefix+'current-release-'+label]
        assert row['exit_code']==0 and row['passed']==count and row['failed']==0 and row['targets']==1
        if label in ('s07','vis38'):assert '--exact' not in row['command']
    py=check.read_bytes(TMP/'vis37-form-gate-vis36-generic-summary-current-python.log')
    assert re.search(rb'Ran 71 tests in .*\n\nOK\n',py)
    workspace={k:v for k,v in records.items() if 'workspace' in k}
    for row in workspace.values():
        assert row['targets']==46 and row['passed']+row['failed']==1874 and row['ignored']==11
        assert '--locked' in row['command'] and '--no-fail-fast' in row['command']
    assert records[prefix+'current-crate-serial']['passed']==536
    return dict(attempts=records,workspace=workspace,release_cases=sum(release.values()),python_tests=71,
        full_workspace_status='PASS' if records[prefix+'current-workspace-serial']['exit_code']==0 else 'NOT_PASS',
        affected_crate_status='PASS' if records[prefix+'current-crate-serial']['exit_code']==0 else 'NOT_PASS',
        interruption=check.load(TMP/'vis36-generic-summary-workspace-interrupted.json'))

def audit(delivery=False):
    before=check.load(BEFORE);preservation(before)
    acquired=check.load(TMP/'vis36-generic-summary-acquisition-007-before.json')
    assert current_source()==acquired['source']
    assert {p:check.elf(Path(p)) for p in acquired['ELF']}==acquired['ELF']
    inverses={}
    for role in (*PROFILES,*(p+'-007' for p in PROFILES),'gates'):
        name='vis36-generic-summary-'+role
        inv=check.load(OUT/(name+'.inventory.json'))
        actual=inverse(OUT/(name+'.tar.xz'),inv)
        assert all(inv[k]==actual[k] for k in ('archive_sha256','archive_bytes','tar_sha256','tar_bytes','xz'))
        assert inv['inventory']==actual['tar']['inventory']
        inverses[role]=actual
    frames={p:whole_frames(p) for p in PROFILES}
    for profile in PROFILES:
        producer.ROOTS=roots(profile);files=producer.source_files()
        assert producer.vis36_prompt_checks(files,profile,'007')==acquired['profiles'][profile]['strict_producer_checks']
        assert {s:effects_ns['effects'](roots(profile)['current'],s) for s in ('oc','upstream')}==acquired['profiles'][profile]['effects']
        del files;gc.collect()
    state=check.load(REPO/'progress/STATE.json')
    if delivery:
        assert state['current']=='T44' and state['tasks']['T44']['last_seq']==144
        assert state['tasks']['T44']['status']=='active'
        assert state['schema_version']==before['state']['schema_version']
        assert {k:v for k,v in state['tasks'].items() if k!='T44'}=={k:v for k,v in before['state']['tasks'].items() if k!='T44'}
        assert check.git('branch','--show-current').decode().strip()=='agent/oc-rust-port'
        assert check.git('remote','get-url','origin').decode().strip()=='git@github.com:0FL01/oc.git'
    result=dict(status='CHECKED_SLICE_NOT_T44_PASS',source_sha256=acquired['source_sha256'],source_count=acquired['source_count'],ELF=acquired['ELF'],gates=gates(),
        profiles=frames,inverses=inverses,prior_proofs=len(before['prior']),legacy_original_files=len(before['original_files']),
        legacy_original_directories=len(before['original_dirs']),quota=check.quota(),memory=check.proc_memory(),state=state)
    print(json.dumps(dict(status=result['status'],source_sha256=result['source_sha256'],profiles={p:{k:v for k,v in f.items() if k not in ('frames','summary_rows')} for p,f in frames.items()},quota=result['quota'])))
    return result

if __name__ == '__main__':
    if sys.argv[1:]==['baseline']:baseline()
    elif sys.argv[1:]==['acquisition']:acquisition()
    elif len(sys.argv)==3 and sys.argv[1]=='frames':save(Path(sys.argv[2]),{p:whole_frames(p) for p in PROFILES})
    elif sys.argv[1:]==['frame-summary']:
        frames=check.load(TMP/'vis36-generic-summary-frames-007-001.json')
        print(json.dumps({p:{k:v for k,v in f.items() if k!='frames'} for p,f in frames.items()},indent=2,ensure_ascii=False))
    elif sys.argv[1:]==['gate-summary']:print(json.dumps(gates(),indent=2))
    elif len(sys.argv)==3 and sys.argv[1] in ('audit','delivery'):save(Path(sys.argv[2]),audit(sys.argv[1]=='delivery'))
    else:raise SystemExit('baseline | acquisition | audit/delivery EXCLUSIVE_REPORT')
