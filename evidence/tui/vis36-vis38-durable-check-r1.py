"""Read-only VIS38 delivery checker; no encoder or historical entrypoint replay."""
import ast
import gc
import importlib.util
import json
from pathlib import Path
import resource
import sys

sys.dont_write_bytecode=True
resource.setrlimit(resource.RLIMIT_AS,(384*1024**2,)*2)
REPO=Path('/home/opencode/ai/oc')
TMP=Path('/home/opencode/.cache/opencode-tmp/opencode')
OUT=REPO/'evidence/tui'
spec=importlib.util.spec_from_file_location('vis38_independent_old_declarations',TMP/'vis38-durable-audit-20261013.py')
audit=importlib.util.module_from_spec(spec);spec.loader.exec_module(audit)
check=audit.check
exec(compile(audit.declarations(OUT/'vis36-vis38-durable-retain.py',{'roots','inventory'}),__file__,'exec'))

def whole_frames():
    root=TMP/'vis38-durable-current-011'
    owner=check.load(root/'dcp-native-vis38-durable-011/result.json')
    identity=check.load(root/'producer-identity.json')
    assert owner['status']=='PASS_ACTUAL_SINGLE_MULTI_RESTART_OWNER_PROBE'
    assert owner['source_HEAD']==identity['actual_producer_HEAD']==audit.BASE
    assert owner['oc_binary_sha256']==identity['ELF'][str(REPO/'target/release/oc')]
    assert owner['provider_requests']==14 and owner['durable_lifecycle']['provider_requests']==1
    assert owner['durable_lifecycle']['restoration_provider_requests']==0
    assert owner['helper_sha256']==check.digest(REPO/'scripts/tui_capture/dcp_native_qualify.py')
    for n,h in owner['evidence_seals'].items():assert check.digest(root/'dcp-native-vis38-durable-011'/n)==h
    frames,grids,pngs=0,0,0
    for lock_path in sorted(root.glob('dcp-*capture*/capture.lock.json'))+sorted(root.glob('dcp-reference*/capture.lock.json')):
        lock=check.load(lock_path);assert lock['attempts'] and all(a['status'].startswith('PASS_') for a in lock['attempts'])
        for capture in lock['captures']:
            prefix=lock_path.parent/capture['name']/capture['stage']
            for ext,h in capture['files'].items():assert check.digest(str(prefix)+'.'+ext)==h
            grid=check.load(str(prefix)+'.cells.json')
            assert grid['columns']==capture['columns'] and grid['rows']==capture['rows']
            assert len(grid['cells'])==grid['rows'] and all(len(r)==grid['columns'] for r in grid['cells'])
            assert grid['producer_commit']==(audit.BASE if grid['origin']=='oc' else '2670273ff17da96f85c5826ced57aa1b368754fa')
            assert all(set(c)=={'symbol','fg','bg','modifiers','width'} for r in grid['cells'] for c in r)
            png=check.read_bytes(str(prefix)+'.png');size,rgba=check.decode_png(png)
            render=check.load(str(prefix)+'.render.json');assert render['unmasked'] and render['cursor']==grid['cursor']
            assert tuple(size)==(render['png_width'],render['png_height'])
            assert render['before']['screen_rect']==render['after']['screen_rect']
            frames+=1;grids+=grid['columns']*grid['rows'];pngs+=size[0]*size[1]
    report=check.load(root/'dcp-pair-check-vis38-durable-011-r2.json')
    native=root/'dcp-nativecapture-dcp-native-vis38-durable-011-recompression'
    original=root/'dcp-reference-vis38-durable-011'
    totals=dict(cells=0,unequal=0,text=0,foreground=0,background=0,modifiers=0,width=0,
        cursor_unequal_frames=0,png_equal_frames=0,png_pixels=0,png_unequal_pixels=0)
    fields={'text':'symbol','foreground':'fg','background':'bg','modifiers':'modifiers','width':'width'}
    for case in report['cases']:
        a,b=[check.load(p/case['name']/'bottom.cells.json') for p in (native,original)]
        counts=dict(cells=a['columns']*a['rows'],unequal=0,**{k:0 for k in fields})
        for ar,br in zip(a['cells'],b['cells'],strict=True):
            for ac,bc in zip(ar,br,strict=True):
                counts['unequal']+=ac!=bc
                for label,key in fields.items():counts[label]+=ac[key]!=bc[key]
        assert counts==case['full_grid'] and (a['cursor']==b['cursor'])==case['cursor']['equal']
        pa,pb=[check.read_bytes(p/case['name']/'bottom.png') for p in (native,original)]
        size,aa=check.decode_png(pa);size2,bb=check.decode_png(pb);assert size==size2
        different=sum(aa[i:i+4]!=bb[i:i+4] for i in range(0,len(aa),4))
        assert different==case['full_png']['unequal_pixels'] and (pa==pb)==case['full_png']['bytes_equal']
        for k,v in counts.items():totals[k]+=v
        totals['cursor_unequal_frames']+=a['cursor']!=b['cursor'];totals['png_equal_frames']+=different==0
        totals['png_pixels']+=size[0]*size[1];totals['png_unequal_pixels']+=different
    assert totals==report['totals'] and totals['unequal']==totals['png_unequal_pixels']==totals['cursor_unequal_frames']==0
    protocol=check.load(root/'dcp-nativecapture-dcp-native-vis38-durable-011-durable/native-durable-120x40/protocol.json')
    states={e['label']:e for e in protocol if e['kind']=='owner_snapshot'}
    assert all(s['read_only'] and s['limit']==100 and all(len(r)<=100 for r in s['data'].values()) for s in states.values())
    def selected(label,table):return [r for r in states[label]['data'][table] if r.get('session_id','s-vis38-actual-owner')=='s-vis38-actual-owner']
    raw=selected('reopened','messages');ops=selected('reopened','tool_operations')
    for label in states:
        current=selected(label,'messages');assert all(r in current for r in raw)
        assert selected(label,'tool_operations')==ops
        if label not in ('branched','branched-dcp'):assert current==raw
    assert selected('redone','dcp_run_views')==selected('reopened','dcp_run_views')
    assert selected('redone','dcp_accounting')==selected('reopened','dcp_accounting')
    assert selected('branched','dcp_accounting')==selected('undo-compression','dcp_accounting')
    assert [json.loads(r['snapshot'])['ordinal'] for r in selected('branched','dcp_run_views')]==[1,2]
    assert not selected('branched','conversation_redo') and selected('branched','conversation_exclusions')
    assert len(selected('branched','messages'))==len(raw)+2
    for p in sorted(root.glob('dcp-nativecapture*/*/protocol.json')):
        protocol=check.load(p);assert any(e['kind']=='exit' and e['code']==0 and e['termination']=='natural' for e in protocol)
    return dict(frames=frames,full_styled_cells=grids,full_png_pixels=pngs,pair_totals=totals,
        owner_requests=owner['provider_requests'],durable_observations=len(states),raw_messages=len(raw),immutable_operations=len(ops))

def archives(before):
    all_members,logical,archives=0,0,[]
    for p in sorted(OUT.glob('vis36-vis38-durable-*.inventory.json')):
        receipt=check.load(p);archive=p.with_name(p.name.replace('.inventory.json','.tar.xz'))
        assert check.digest(archive)==receipt['archive_sha256']
        check.ROOTS={k:Path(v) for k,v in receipt['source_roots'].items()}
        raw,xz=check.inverse_xz(check.read_bytes(archive));restored,tar=check.inverse_ustar(raw,receipt['original_members'])
        assert xz==receipt['xz'] and tar['inventory']==receipt['inventory'] and check.sha(raw)==receipt['tar_sha256']
        assert len(raw)==receipt['tar_bytes'] and len(restored)==len(receipt['original_members'])
        for r in receipt['original_members'].values():assert check.metadata(r['path'])==r['original_metadata']
        for d,m in receipt['original_directories'].items():assert check.metadata(d)==m
        archives.append(dict(name=archive.name,sha256=receipt['archive_sha256'],bytes=receipt['archive_bytes']))
        all_members+=len(restored);logical+=tar['inventory']['logical_bytes']
        del restored,raw;gc.collect()
    return dict(archives=archives,members=all_members,logical_bytes=logical)

def main():
    before=check.load(audit.BEFORE);audit.preservation(before)
    sealed=check.load(TMP/'vis38-durable-acquisition-before-20261013.json')
    assert audit.current_source()==sealed['source']
    assert check.producer_sha(sealed['source'])=='ced0ae5ba472051f8a61e7e0378249cc4d81194368b6088fc79c061af9ffbf6a'
    for p,h in sealed['ELF'].items():assert check.digest(p)==h
    for attempt,expected in sealed['attempts'].items():assert inventory(roots(attempt))==expected
    raw,xz=check.inverse_xz(check.read_bytes(OUT/'model-shell-tool-regression-002.tar.xz'))
    inverse_pax=audit.inverse_pax
    inverse_pax(raw,before['selected']);del raw;gc.collect()
    frames=whole_frames();packed=archives(before)
    state=check.load(REPO/'progress/STATE.json')
    if '--delivery' in sys.argv:
        assert state['current']=='T44' and state['tasks']['T44']['status']=='active'
        assert state['tasks']['T44']['last_seq']==143 and Path(state['tasks']['T44']['latest']).name=='0143.md'
        assert len(packed['archives'])==12
        assert not check.git('diff','--name-only','--','GOAL.md','planning/tasks.json','tui-recovery/ACCEPTANCE.json','tui-recovery/T44_CONTRACT_AMENDMENT.md')
    print(json.dumps(dict(status='PASS_READONLY_FULL_BYTE_CURRENT_VIS38_SLICE_NOT_T44_PASS',
        actual_HEAD=check.git('rev-parse','HEAD').decode().strip(),source_count=len(sealed['source']),source_sha256=sealed['source_sha256'],ELF=sealed['ELF'],
        preserved_prior=len(before['prior']),preserved_files=len(before['original_files']),preserved_directories=len(before['original_dirs']),
        frames=frames,retention=packed,quota=check.quota(),memory=check.proc_memory(),active=state['current'],checkpoint=state['tasks']['T44']['last_seq'])))

if __name__=='__main__':main()
