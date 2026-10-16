"""VIS38 exclusive full-byte acquisition/gate retention; existing encoder only.

Manual single-XZ/USTAR/PAX inverse declarations are loaded independently of the
encoder; every original byte/path and separate original DIR metadata is sealed.
"""
import gc
import importlib.util
import json
import os
from pathlib import Path
import resource
import stat
import sys

sys.dont_write_bytecode=True
resource.setrlimit(resource.RLIMIT_AS,(384*1024**2,)*2)
REPO=Path('/home/opencode/ai/oc')
TMP=Path('/home/opencode/.cache/opencode-tmp/opencode')
OUT=REPO/'evidence/tui'

def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path)
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value)
    return value

audit=load('vis38_manual_declarations',TMP/'vis38-durable-audit-20261013.py')
check=audit.check
storage=load('vis38_quota_existing',REPO/'scripts/archive_failed_capture.py')

def quota():
    assert 'vis36-' in storage.PREFIXES
    n=storage.owned_allocation();assert n<=storage.CAP
    return dict(allocated_bytes=n,cap_bytes=storage.CAP,headroom_bytes=storage.CAP-n)

def roots(attempt):
    name=f'vis38-durable-current-{attempt}'
    root=TMP/name
    assert root.is_dir()
    found={'acquisition':root}
    parent=TMP/'t44-reference/runs'
    for p in sorted(parent.glob(f'dcp-*-vis38-durable-{attempt}*')):
        assert p.is_dir() and not p.is_symlink()
        found['runtime-'+str(len(found))]=p
    return found

def inventory(mapping):
    files,dirs={},{}
    for key,root in mapping.items():
        paths=[root,*sorted(root.rglob('*'))] if root.is_dir() else [root]
        for p in paths:
            m=p.lstat();assert m.st_uid==1003 and not p.is_symlink()
            name=key+'/'+p.relative_to(root).as_posix() if p!=root else key
            if stat.S_ISDIR(m.st_mode):dirs[str(p)]=check.metadata(p)
            else:
                assert stat.S_ISREG(m.st_mode) and m.st_size<=16*1024**2
                if key=='acquisition':
                    relative=p.relative_to(root).as_posix()
                    if relative.startswith('dcp-nativecapture-'):
                        stage=relative.split('/')[0].split(root.name[-3:]+'-',1)[1]
                        relative=relative.replace(relative.split('/')[0],'oc/'+stage,1)
                        for profile in ('80x24','120x40','160x48'):
                            relative=relative.replace('native-'+stage+'-'+profile,profile)
                    elif relative.startswith('dcp-reference-'):
                        relative=relative.replace(relative.split('/')[0],'upstream/recompression',1)
                        relative=relative.replace('native-recompression-','')
                    else:
                        for prefix,short in (('dcp-native-','owner'),('dcp-oracle-','oracle'),('dcp-display-','display')):
                            if relative.startswith(prefix):relative=relative.replace(relative.split('/')[0],short,1)
                    name=key+'/'+relative
                assert len(name.encode('ascii'))<100,name
                files[name]=dict(path=str(p),bytes=m.st_size,sha256=check.digest(p),original_metadata=check.metadata(p))
    return dict(files=files,directories=dirs)

def scopes():
    return {p.name[-3:]:inventory(roots(p.name[-3:])) for p in sorted(TMP.glob('vis38-durable-current-[0-9][0-9][0-9]'))}

def measure():
    selected=scopes()
    facts={a:dict(members=len(v['files']),logical_bytes=sum(r['bytes'] for r in v['files'].values()),
        largest_member=max(r['bytes'] for r in v['files'].values()),directories=len(v['directories'])) for a,v in selected.items()}
    print(json.dumps(dict(attempts=facts,combined_members=sum(v['members'] for v in facts.values()),
        combined_logical_bytes=sum(v['logical_bytes'] for v in facts.values()),quota=quota(),memory=check.proc_memory())))

def seal_before():
    before=check.load(audit.BEFORE);audit.preservation(before)
    selected=scopes()
    record=dict(version=1,attempts=selected,source=audit.current_source(),
        source_sha256=check.producer_sha(audit.current_source()),
        ELF={str(p):check.digest(p) for p in (REPO/'target/debug/oc',REPO/'target/release/oc',TMP/'t44-reference/package/bin/opencode')},
        historical_baseline_sha256=check.digest(audit.BEFORE),before_quota=quota())
    audit.save(TMP/'vis38-durable-acquisition-before-20261013.json',record)

def inverse(encoded,expected):
    raw,xz=check.inverse_xz(encoded)
    restored,tar=check.inverse_ustar(raw,expected)
    assert len(restored)==len(expected)
    for name,row in expected.items():assert len(restored[name])==row['bytes'] and check.sha(restored[name])==row['sha256']
    return xz,tar,restored

def pack(name,expected,dirs,mapping):
    assert len(expected)<=2000 and sum(r['bytes'] for r in expected.values())<=128*1024**2
    check.ROOTS=mapping
    files={n:check.read_bytes(r['path']) for n,r in expected.items()}
    producer=load('vis38_existing_encoder_only',REPO/'scripts/retain_syntax_proof.py')
    producer.ROOTS=mapping
    first,aliases={},{}
    for n in sorted(files):
        if n.endswith('.png'):
            key=(tuple(n.split('/')[:2]),check.sha(files[n]))
            if key in first:
                assert files[n]==files[first[key]] and n.split('/')[1] in ('oc','upstream')
                aliases[n]=first[key]
            else:first[key]=n
    encoded,raw_sha,raw_bytes=producer.encode(files,aliases)
    assert len(encoded)<=16*1024**2 and raw_bytes<=128*1024**2
    xz,tar,restored=inverse(encoded,expected)
    assert restored==files and tar['inventory']['same_side_png_aliases']==len(aliases)
    del restored,files;gc.collect()
    archive,receipt=OUT/(name+'.tar.xz'),OUT/(name+'.inventory.json')
    assert not archive.exists() and not receipt.exists()
    record=dict(version=1,codec='encoder-only canonical USTAR / single SHA256-XZ; independent manual inverse',
        archive_sha256=check.sha(encoded),archive_bytes=len(encoded),tar_sha256=raw_sha,tar_bytes=raw_bytes,
        inventory=tar['inventory'],xz=xz,original_members=expected,original_directories=dirs,
        source_roots={k:str(v) for k,v in mapping.items()},helper_sha256=check.digest(Path(__file__)),
        command=['python3','-B',str(Path(__file__)),*sys.argv[1:]],memory=check.proc_memory())
    data=(json.dumps(record,indent=2)+'\n').encode();assert len(data)<=16*1024**2
    assert quota()['headroom_bytes']>=sum((len(d)+4095)//4096*4096 for d in (encoded,data))+1048576
    storage.durable(archive,encoded);storage.durable(receipt,data)
    xz2,tar2,restored=inverse(check.read_bytes(archive),expected)
    assert xz2==xz and tar2==tar
    del restored;gc.collect()
    for row in expected.values():assert check.metadata(row['path'])==row['original_metadata'] and check.digest(row['path'])==row['sha256']
    for p,m in dirs.items():assert check.metadata(p)==m
    print(json.dumps(dict(status='FULL_MANUAL_INVERSE_MEMORY_AND_DURABLE',archive=str(archive),
        archive_sha256=record['archive_sha256'],archive_bytes=len(encoded),inventory=tar['inventory'],quota=quota(),memory=check.proc_memory())))

def acquisition(attempt):
    before=check.load(TMP/'vis38-durable-acquisition-before-20261013.json')
    mapping=roots(attempt);current=inventory(mapping)
    assert current==before['attempts'][attempt]
    pack('vis36-vis38-durable-'+attempt,current['files'],current['directories'],mapping)

def gates():
    paths=[*sorted(TMP.glob('vis37-form-gate-vis38-durable-*.*')),
        *sorted(TMP.glob('vis38-durable-*.py')),
        *sorted(TMP.glob('vis38-durable-*.json')),TMP/'vis37-form-gate-20261012.py',
        Path(__file__),*sorted(OUT.glob('vis36-vis38-durable-check*.py'))]
    assert len(paths)==len(set(paths))
    mapping={'gates':TMP,'programs':OUT}
    expected={('gates/' if p.parent==TMP else 'programs/')+p.name:dict(path=str(p),bytes=p.stat().st_size,
        sha256=check.digest(p),original_metadata=check.metadata(p)) for p in paths}
    pack('vis36-vis38-durable-gates',expected,{},mapping)

if __name__=='__main__':
    if sys.argv[1]=='measure':measure()
    elif sys.argv[1]=='before':seal_before()
    elif sys.argv[1]=='gates':gates()
    else:
        assert sys.argv[1]=='pack' and len(sys.argv)==3
        acquisition(sys.argv[2])
