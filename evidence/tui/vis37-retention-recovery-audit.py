"""Independent sealed-before/manual PAX+USTAR inverse; no producer imports."""
import sys
sys.dont_write_bytecode = True
from decimal import Decimal
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import resource
import stat

resource.setrlimit(resource.RLIMIT_AS, (384 * 1024**2, 384 * 1024**2))
REPO = Path('/home/opencode/ai/oc')
TMP = Path('/home/opencode/.cache/opencode-tmp/opencode')
OUT = REPO / 'evidence/tui'
spec = importlib.util.spec_from_file_location('immutable_form_observer', TMP / 'vis37-form-independent-20261012-sole-verifier/owner_refinement.py')
observer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(observer)
v, check = observer.v, observer.v.inverse
v.ROOTS = {**{'current-' + p: TMP / 'vis37-form-current-004' / p for p in ('small', 'wide', 'tall')},
           'current-logs': TMP / 'vis37-form-current-004', 'current-driver': TMP / 'vis37-form-capture-r4.py'}
check.ROOTS = v.ROOTS
BASE = 'd376d12fdd7c245df00194bc62611e8114c0d88c'
BEFORE = OUT / 'vis37-retention-recovery-before.json'
AFTER = OUT / 'vis37-retention-recovery-after.json'


def save(path, value):
    data = (json.dumps(value, indent=2) + '\n').encode()
    check.require(len(data) <= check.FILE and check.quota()['headroom_bytes'] >= (len(data) + 4095) // 4096 * 4096,
                  'bounded report within unchanged allocation cap')
    with path.open('xb') as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    print(json.dumps({'output': str(path), 'bytes': len(data), 'sha256': check.sha(data),
                      'status': value['status'], 'memory': check.proc_memory(), 'quota': check.quota()}))


def diagnostic_snapshot():
    root = OUT / 'model-shell-001'
    files, directories = {}, {}
    for path in sorted([root, *root.rglob('*')]):
        meta = path.lstat()
        check.require(meta.st_uid == os.getuid() and not path.is_symlink(), 'owned diagnostic member')
        name = path.relative_to(OUT).as_posix()
        values = [stat.S_IMODE(meta.st_mode), meta.st_uid, meta.st_gid, meta.st_mtime_ns]
        if stat.S_ISDIR(meta.st_mode):
            directories[name] = values
        else:
            check.require(stat.S_ISREG(meta.st_mode) and meta.st_nlink == 1 and meta.st_size <= check.FILE,
                          'bounded regular diagnostic without external hard links')
            files[name] = {'bytes': meta.st_size, 'sha256': check.digest(path), 'metadata': values}
    check.require(len(files) + len(directories) <= check.MEMBERS and sum(r['bytes'] for r in files.values()) <= check.TOTAL,
                  'whole selected diagnostic within unchanged bounds')
    return {'files': files, 'directories': directories}


def source_and_elf():
    source = v.source()
    check.require(len(source) == 713, 'all actual713 source inputs')
    facts = check.load(TMP / 'vis37-form-r4-independent.json')
    for profile in ('small', 'wide', 'tall'):
        episode = v.ROOTS['current-' + profile]
        lock, manifest = (check.load(episode / n) for n in ('capture.lock.json', 'source-manifest.json'))
        check.require(manifest == source and check.producer_sha(source) == lock['oc']['source_manifest_sha256'],
                      'recorded source-current manifest, not rewritten HEAD identity')
        for side in ('oc', 'upstream'):
            check.require(check.digest(lock[side]['executable_path']) == lock[side]['executable_sha256'] == facts['elf'][side]['sha256'],
                          'same actual captured native/reference ELF')
    return {'source_manifest_sha256': check.producer_sha(source), 'elf': facts['elf']}


def immutable_helpers():
    names = [Path(__file__), TMP / 'vis37-form-independent-20261012-sole-verifier/verifier.py',
             TMP / 'vis37-form-independent-20261012-sole-verifier/owner_refinement.py',
             *sorted(v.OLD_HELPERS.glob('*.py'))]
    return {str(p): check.digest(p) for p in names}


def inverse_pax(raw, expected):
    """Parse the limited real PAX framing directly, without tarfile/producer audit."""
    offset, pending, files, directories = 0, {}, {}, {}
    while offset + 512 <= len(raw) and any(raw[offset:offset + 512]):
        header = raw[offset:offset + 512]
        number = lambda start, width: int(header[start:start + width].strip(b'\0 '), 8)
        checksum = sum(header[:148]) + 8 * 32 + sum(header[156:])
        check.require(number(148, 8) == checksum and header[257:265] == b'ustar\00000', 'whole PAX/USTAR header/checksum')
        name = header[:100].split(b'\0', 1)[0].decode('utf-8')
        prefix = header[345:500].split(b'\0', 1)[0].decode('utf-8')
        name = prefix + '/' + name if prefix else name
        size, kind = number(124, 12), header[156:157]
        offset += 512
        end, padded = offset + size, offset + (size + 511) // 512 * 512
        check.require(size <= check.FILE and padded <= len(raw) and not any(raw[end:padded]), 'complete bounded payload/zero padding')
        data = raw[offset:end]
        offset = padded
        if kind == b'x':
            check.require(not pending, 'one local PAX header per actual member')
            cursor = 0
            while cursor < len(data):
                space = data.index(b' ', cursor)
                length = int(data[cursor:space])
                finish = cursor + length
                check.require(space < finish <= len(data) and data[finish - 1:finish] == b'\n', 'exact length-prefixed PAX record')
                key, value = data[space + 1:finish - 1].split(b'=', 1)
                check.require(key in (b'mtime', b'path') and key not in pending, 'only real local metadata keys, no duplicates')
                pending[key] = value.decode('utf-8')
                cursor = finish
            continue
        check.require(kind in (b'0', b'5') and b'mtime' in pending and not any(header[157:257]), 'regular/directory, no links or specials')
        name = pending.get(b'path', name).removesuffix('/') if kind == b'5' else pending.get(b'path', name)
        path = PurePosixPath(name)
        check.require(not path.is_absolute() and path.as_posix() == name and '\0' not in name and
                      all(p not in ('', '.', '..') for p in name.split('/')) and path.parts[0] == 'model-shell-001', 'confined original path')
        timestamp = Decimal(pending[b'mtime']) * 1_000_000_000
        check.require(timestamp == int(timestamp), 'exact integer nanosecond metadata')
        metadata = [number(100, 8), number(108, 8), number(116, 8), int(timestamp)]
        pending = {}
        check.require(name not in files and name not in directories and len(files) + len(directories) < check.MEMBERS, 'complete unique member set')
        if kind == b'5':
            check.require(size == 0 and expected['directories'].get(name) == metadata, 'exact original directory metadata')
            directories[name] = metadata
        else:
            record = expected['files'].get(name)
            check.require(record is not None and record['bytes'] == size and record['sha256'] == check.sha(data) and
                          record['metadata'] == metadata, 'every original byte/hash and file metadata')
            files[name] = record
    check.require(not pending and set(files) == set(expected['files']) and directories == expected['directories'], 'complete58 files/3 directories, no omissions/additions')
    check.require(len(raw) % 10240 == 0 and len(raw) - offset >= 1024 and not any(raw[offset:]), 'two TAR end blocks and only zero record padding')
    return {'files': len(files), 'directories': len(directories), 'logical_bytes': sum(r['bytes'] for r in files.values()),
            'metadata': 'mode/uid/gid/mtime_ns; original paths and all encoded bytes', 'manual_pax_header_and_record_reader': True}


check.require(os.getuid() == 1003 and check.git('rev-parse', 'HEAD').decode().strip() == BASE, 'dedicated owner and current delivery base')
mode, = sys.argv[1:]
facts = check.load(TMP / 'vis37-form-r4-independent.json')
current_inventory = v.inventory()
check.require(current_inventory == facts['original_inventory'], 'all004 original bytes and directory metadata still sealed')
if mode == 'before':
    prior = {str(p): {'sha256': check.digest(p), 'metadata': check.metadata(p)}
             for p in sorted(OUT.glob('*.tar.xz'))}
    prior.update({str(p): {'sha256': check.digest(p), 'metadata': check.metadata(p)}
                  for p in sorted(OUT.glob('*.inventory.json'))})
    check.require(not (OUT / 'model-shell-001.tar.xz').exists() and not (OUT / 'vis37-form-r4-proof.tar.xz').exists(), 'exclusive new outputs absent')
    report = {'status': 'INDEPENDENT_BEFORE_RETENTION_RECOVERY_T44_ACTIVE_NOT_PASS',
              'diagnostic': diagnostic_snapshot(), 'current_inventory_sha256': check.canonical_sha(current_inventory),
              'current_recount_sha256': check.digest(TMP / 'vis37-form-r4-independent.json'),
              'source_and_elf': source_and_elf(), 'preserved_prior_proofs': prior, 'helpers': immutable_helpers(),
              'quota': check.quota(), 'memory': check.proc_memory()}
    save(BEFORE, report)
elif mode == 'after':
    before = check.load(BEFORE)
    check.require(check.canonical_sha(current_inventory) == before['current_inventory_sha256'] and
                  check.digest(TMP / 'vis37-form-r4-independent.json') == before['current_recount_sha256'], 'sealed004 originals and recount unchanged')
    check.require(source_and_elf() == before['source_and_elf'] and immutable_helpers() == before['helpers'], 'source/ELFs/helpers preserved')
    for path, row in before['preserved_prior_proofs'].items():
        check.require(check.digest(path) == row['sha256'] and check.metadata(path) == row['metadata'], 'all prior retained archives/inventories unchanged')
    diagnostic = OUT / 'model-shell-001.tar.xz'
    diagnostic_receipt = check.load(OUT / 'model-shell-001.archived.json')
    raw, xz = check.inverse_xz(check.read_bytes(diagnostic))
    diagnostic_inverse = inverse_pax(raw, before['diagnostic'])
    check.require(check.digest(diagnostic) == diagnostic_receipt['archive_sha256'] and
                  diagnostic_receipt['statuses_unchanged'] == {'oc': 'FAILED', 'upstream': 'PASS_BEHAVIOR_ONLY'}, 'durable failed status/encoded identity')
    note = check.read_bytes(OUT / 'model-shell-001/ARCHIVED.md')
    check.require(b'FAILED / NOT_QUALIFIED' in note and len(list((OUT / 'model-shell-001').iterdir())) == 1, 'failure-visible original-path pointer only')
    del raw
    archive = OUT / 'vis37-form-r4-proof.tar.xz'
    receipt = check.load(OUT / 'vis37-form-r4-proof.inventory.json')
    raw, xz004 = check.inverse_xz(check.read_bytes(archive))
    restored, result = check.inverse_ustar(raw, current_inventory['files'])
    check.require(check.digest(archive) == receipt['archive_sha256'] and check.sha(raw) == receipt['tar_sha256'] and
                  result['inventory'] == receipt['inventory'], 'manual current004 inverse and frozen whole inventory')
    effects = {profile + '-' + side: v.current_facts(v.ROOTS['current-' + profile], side)
               for profile in ('small', 'wide', 'tall') for side in ('oc', 'upstream')}
    check.require(current_inventory == v.inventory() and source_and_elf() == before['source_and_elf'], 'all004 original bytes/metadata and source remained unchanged during inverse')
    report = {'status': 'INDEPENDENT_RETENTION_UNBLOCKED_FULL_BYTE_EXACT_T44_ACTIVE_NOT_PASS',
              'before_sha256': check.digest(BEFORE), 'helper_sha256': check.digest(__file__),
              'diagnostic_inverse': diagnostic_inverse, 'diagnostic_archive_sha256': check.digest(diagnostic), 'diagnostic_xz': xz,
              'current004_inverse': result['inventory'], 'current004_archive_sha256': check.digest(archive), 'current004_xz': xz004,
              'current_source_and_elf': before['source_and_elf'], 'effects': effects,
              'unchanged_independent_full_frame_totals': facts['totals'], 'prior_proof_count': len(before['preserved_prior_proofs']),
              'memory': check.proc_memory(), 'quota': check.quota(),
              'scope': 'Every004 byte inverse verified; prior exact full-frame recount preserved, not rerun or promoted. All full comparisons DIFFERENT; DCP tables unread; final generation1 forced. No VIS37/R6/V09/T44 pixel PASS.'}
    save(AFTER, report)
else:
    raise ValueError('expected before/after')
