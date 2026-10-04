#!/usr/bin/env python3
"""Read only this atomic's compressed receipts; never mutate retained logs."""
import gzip
import hashlib
import json
from pathlib import Path
import re
import sys
import subprocess
import sqlite3

from run_background_check import BENCH, ROOT


def main():
    if '--repair-timeout' in sys.argv:
        completed = (BENCH/'t45-background-repair-green1.log.gz').stat().st_mtime
        for path in BENCH.iterdir():
            if not path.name.startswith('.tmp') or not path.is_dir():
                continue
            if not completed-1775 <= path.stat().st_mtime <= completed-1700:
                continue
            item = dict(path=str(path), entries=sorted(child.name for child in path.iterdir()))
            if (path/'mcp.pid').exists():
                pid = int((path/'mcp.pid').read_text())
                item.update(owned_retirement_leaf=pid, leaf_absent=not Path(f'/proc/{pid}').exists())
            if (path/'oc.sqlite').exists():
                with sqlite3.connect(f'file:{path}/oc.sqlite?mode=ro', uri=True) as db:
                    item.update(sessions=db.execute('SELECT count(*) FROM sessions').fetchone()[0],
                        jobs=db.execute('SELECT count(*) FROM child_jobs').fetchone()[0],
                        child_migration=db.execute('SELECT version,applied_at FROM schema_migrations WHERE version=10').fetchall())
            print(json.dumps(item))
        return
    if '--aborted-fixture' in sys.argv:
        completed = (BENCH/'t45-background-app2.log.gz').stat().st_mtime
        candidates = []
        for path in BENCH.iterdir():
            if path.name.startswith('.tmp') and path.is_dir() and completed-1800 <= path.stat().st_mtime <= completed:
                candidates.append(dict(path=str(path), modified=path.stat().st_mtime,
                    entries=sorted(child.name for child in path.iterdir())))
        owned = BENCH/'.tmpEKnryJ'
        if owned.exists():
            config = json.loads((owned/'source/opencode.json').read_text())
            assert config['agent']['helper']['system'] == 'OWN_BACKGROUND_PROFILE'
            assert config['provider']['fixture']['options']['apiKey'] == 'synthetic'
            with sqlite3.connect(f'file:{owned}/data/oc.sqlite?mode=ro', uri=True) as db:
                print(json.dumps(dict(owned_path=str(owned), profile_confirmed=True,
                    turns=db.execute('SELECT session_id,prompt,status FROM turns').fetchall(),
                    tool_effect_intents=db.execute("SELECT count(*) FROM tool_operations WHERE name!='subagent'").fetchone()[0],
                    jobs=db.execute('SELECT count(*) FROM child_jobs').fetchone()[0],
                    runner_reaped=not Path('/proc/3608777').exists())))
        print(json.dumps(dict(app2_completed=completed, candidates=candidates)))
        return
    total = 0
    owners = set()
    removed = set()
    exclude = {argument.split('=', 1)[1] for argument in sys.argv if argument.startswith('--exclude-log=')}
    selected = {argument.split('=', 1)[1] for argument in sys.argv if argument.startswith('--log=')}
    for path in sorted(BENCH.glob('t45-background-*.log.gz')):
        if path.name in exclude or (selected and path.name not in selected):
            continue
        raw = gzip.decompress(path.read_bytes())
        total += path.stat().st_size
        body, marker, receipt = raw.rpartition(b'\nRECEIPT ')
        if marker:
            info = json.loads(receipt)
            assert hashlib.sha256(body).hexdigest() == info['raw_sha256']
            assert len(body) == info['raw_bytes']
            info['pid_absent_now'] = not Path(f"/proc/{info['owner']['pid']}").exists()
            owners.add(info['owner']['pid'])
        else:
            body = raw
            text = body.decode(errors='replace')
            info = dict(raw_sha256=hashlib.sha256(body).hexdigest(),
                raw_bytes=len(body), legacy=True,
                counts=re.findall(r'test result: .*', text),
                metadata=[line for line in text.splitlines() if line.startswith(('EXIT=', 'ARGV=', 'HEAD=', 'SOURCE=', 'OWNER=', 'RECEIPT'))])
        info.update(log=str(path), retained_bytes=path.stat().st_size)
        if '--details' in sys.argv:
            print(json.dumps(info))
            text = body.decode(errors='replace')
            for section in text.split('\nfailures:\n')[1:]:
                print(section.split('\ntest result:')[0])
            continue
        if '--cleanup' in sys.argv:
            for line in body.decode(errors='replace').splitlines():
                fixture = re.search(r'owned retirement fixture data=(\S+) project=(\S+) leaf=(\d+)', line)
                if fixture:
                    assert all(Path(path).parent == BENCH for path in fixture.group(1, 2))
                    removed.update(fixture.group(1, 2))
                    owners.add(int(fixture.group(3)))
                try:
                    item = json.loads(line)
                except (ValueError, TypeError):
                    continue
                if not isinstance(item, dict):
                    continue
                owners.update(owner['pid'] for owner in item.get('owners', []))
                if item.get('leaf'):
                    owners.add(item['leaf']['pid'])
                if item.get('cleanup_path'):
                    removed.add(item['cleanup_path'])
            continue
        if '--budget' in sys.argv:
            continue
        if '--native' in sys.argv and not selected and path.name != 't45-background-native14.log.gz':
            continue
        if '--failures' in sys.argv and not info.get('legacy') and info['exit'] == 0:
            continue
        print(json.dumps(info))
        if ('--native' in sys.argv and selected) or path.name in ('t45-background-native14.log.gz', 't45-background-python47c.log.gz',
                         't45-background-docs-final.log.gz', 't45-background-progress-final2.log.gz', 't45-background-size-final.log.gz'):
            if 'native' not in path.name:
                print(json.dumps(dict(output=body.decode())))
                continue
            for line in body.decode().splitlines():
                item = json.loads(line)
                if 'case' not in item:
                    print(json.dumps(item))
                    continue
                counts = item['counts']
                print(json.dumps(dict(binary=item['binary'], case=item['case'], status=item['status'],
                    physical=counts['physical'], main=counts['main'], auxiliary=counts['auxiliary'],
                    jobs=len(counts.get('jobs', [])), notices=counts.get('notices', 0), operations=len(counts['operations']),
                    effect_operations=counts.get('effect_operations'), child_effects=item.get('child_effects', item.get('effects')),
                    parent_effects=item.get('parent_effects', 0), owners=item['owners'], leaf=item.get('leaf'),
                    owned_start_order=counts.get('owned_start_order'),
                    native_owners_absent=all(not Path(f"/proc/{owner['pid']}").exists() for owner in item['owners']))))
    print(json.dumps(dict(retained_gzip_bytes=total)))
    if '--cleanup' in sys.argv:
        remaining = sorted(pid for pid in owners if Path(f'/proc/{pid}').exists())
        retained = sorted(path for path in removed if Path(path).exists())
        aborted = ['.tmpEKnryJ', '.tmppKLv2E', '.tmp25RSxz']
        assert not remaining and not retained and all(not (BENCH/name).exists() for name in aborted)
        native_trees = [str(path) for path in BENCH.glob('t50-background-owned-*')]
        assert not native_trees
        print(json.dumps(dict(checked_owned_pids=len(owners), live_owned_pids=remaining,
            checked_exact_native_tempdirs=len(removed), retained_exact_tempdirs=retained,
            aborted_rust_tempdirs_removed=aborted, retained_native_fixture_trees=native_trees)))
        hashes = {}
        for name in ('target/debug/oc', 'target/release/oc'):
            raw = (ROOT/name).read_bytes()
            assert raw[:4] == b'\x7fELF'
            hashes[name] = hashlib.sha256(raw).hexdigest()
        normal = [argument.split('=', 1)[1] for argument in sys.argv if argument.startswith('--normal-log=')]
        if normal:
            assert len(normal) == 1 and Path(normal[0]).name == normal[0]
            proof = gzip.decompress((BENCH/normal[0]).read_bytes()).rpartition(b'\nRECEIPT ')[0]
            records = [json.loads(line) for line in proof.decode().splitlines()]
            expected = records[0]['before']
            assert records[-1]['after'] == expected
            assert {str(ROOT/name):value for name,value in hashes.items()} == expected
        else:
            assert hashes == {'target/debug/oc':'14529faf67653e151709a080ef232b1898fa1e1ed2bd1c67bc630a1074443a42',
                'target/release/oc':'71ba7f905e715996a6fdf2cfc8ce05348206fd441938556125e43ff9cb7e4e52'}
        print(json.dumps(dict(retained_normal_elf_sha256=hashes)))
    if '--budget' in sys.argv:
        files = ['background-children.md', 'native_background_children.py',
                 'run_background_check.py', 'inspect_background_receipts.py']
        new = sum((ROOT/'evidence/T45'/name).stat().st_size for name in files)
        evidence = sum(path.stat().st_size for path in (ROOT/'evidence').rglob('*') if path.is_file())
        tracked = subprocess.check_output(['git', 'ls-files', '-z', '--', 'evidence'], cwd=ROOT).decode().split('\0')
        tracked_bytes = sum((ROOT/path).stat().st_size for path in tracked if path and (ROOT/path).is_file())
        print(json.dumps(dict(new_repository_evidence_bytes=new,
            atomic_with_logs_bytes=new+total, all_local_evidence_bytes=evidence,
            tracked_repository_evidence_bytes=tracked_bytes)))


if __name__ == '__main__':
    main()
