"""Exclusive full original-byte retention, strict Prompt producer admission.

Encoder only; every original path/byte/metadata is checked by the independent
manual single-XZ/canonical-USTAR inverse before and after durable publication.
"""
import gc
import importlib.util
import json
from pathlib import Path
import sys

sys.dont_write_bytecode=True
HERE=Path(__file__).parent
spec=importlib.util.spec_from_file_location('generic_summary_observer',HERE/'vis36-generic-summary-check.py')
audit=importlib.util.module_from_spec(spec);spec.loader.exec_module(audit)
check,producer,OUT,TMP=audit.check,audit.producer,audit.OUT,audit.TMP
storage=audit.module('generic_summary_storage',audit.REPO/'scripts/archive_failed_capture.py')

def pack(role,inv,files):
    mapping={k:Path(v) for k,v in inv['source_roots'].items()}
    producer.ROOTS=check.ROOTS=mapping
    aliases=producer.png_aliases(files)
    encoded,tar_sha,tar_bytes=producer.encode(files,aliases)
    assert len(encoded)<=check.FILE and tar_bytes<=check.TOTAL
    raw,xz=check.inverse_xz(encoded)
    restored,tar=check.inverse_ustar(raw,inv['original_members'])
    assert restored==files and check.sha(raw)==tar_sha and tar['inventory']==producer.inventory(files,aliases)
    del raw,restored,files;gc.collect()
    name='vis36-generic-summary-'+role
    archive,receipt=OUT/(name+'.tar.xz'),OUT/(name+'.inventory.json')
    assert not archive.exists() and not receipt.exists()
    record={**inv, 'version':1,'codec':'encoder-only single SHA256-XZ/canonical USTAR; independent manual inverse',
        'archive_sha256':check.sha(encoded),'archive_bytes':len(encoded),'tar_sha256':tar_sha,'tar_bytes':tar_bytes,
        'xz':xz,'inventory':tar['inventory'],'helper_sha256':check.digest(Path(__file__)),
        'command':['python3','-B',str(Path(__file__)),*sys.argv[1:]],'memory':check.proc_memory()}
    data=(json.dumps(record,indent=2)+'\n').encode();assert len(data)<=check.FILE
    assert check.quota()['headroom_bytes']>=sum((len(d)+4095)//4096*4096 for d in (data,encoded))+1048576
    storage.durable(archive,encoded);storage.durable(receipt,data)
    actual=audit.inverse(archive,record)
    assert all(actual[k]==record[k] for k in ('archive_sha256','archive_bytes','tar_sha256','tar_bytes','xz'))
    assert actual['tar']['inventory']==record['inventory']
    print(json.dumps(dict(status='DURABLE_FULL_MANUAL_INVERSE',archive=str(archive),archive_sha256=record['archive_sha256'],inventory=record['inventory'],quota=check.quota(),memory=check.proc_memory())))

def acquisition(profile):
    before=check.load(TMP/'vis36-generic-summary-acquisition-007-before.json')
    mapping=audit.roots(profile);producer.ROOTS=mapping
    files=producer.source_files();producer.vis36_prompt_checks(files,profile,'007')
    inv=audit.inventory(mapping,files)
    assert all(inv[k]==before['profiles'][profile][k] for k in inv)
    pack(profile+'-007',inv,files)

def gates():
    paths=[*sorted(TMP.glob('vis37-form-gate-vis36-generic-summary-*.*')),
        *sorted(TMP.glob('vis36-generic-summary-*.py')),
        *sorted(TMP.glob('vis36-generic-summary-*.json')),
        TMP/'vis36-prompt-capture-r6.py',TMP/'vis37-form-gate-20261012.py',Path(__file__),HERE/'vis36-generic-summary-check.py']
    assert len(paths)==len(set(paths))
    mapping={'gates':TMP,'programs':HERE}
    files={('gates/' if p.parent==TMP else 'programs/')+p.name:check.read_bytes(p) for p in paths}
    inv=audit.inventory(mapping,files)
    # Parent evidence and TMP metadata change when publishing exclusive reports;
    # seals cover every original member and all acquisition DIRs, not these parents.
    inv['original_directories']={}
    pack('gates',inv,files)

if __name__=='__main__':
    assert len(sys.argv)==2 and sys.argv[1] in ('wide','small','gates')
    if sys.argv[1]=='gates':gates()
    else:acquisition(sys.argv[1])
