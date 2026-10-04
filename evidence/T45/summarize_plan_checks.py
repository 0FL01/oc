#!/usr/bin/env python3
"""Read only this slice's exact private gzip receipts (no other campaigns)."""
import gzip
from collections import Counter
import json
from pathlib import Path
import re
import sys

BENCH = Path('/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924')

def main():
    names = sys.argv[1:]
    if names == ['--aggregate']:
        text = gzip.decompress((BENCH/'t45-plan-native-verified2.log.gz').read_bytes()).decode()
        totals = {}
        for line in text.splitlines():
            if not line.startswith('{'): continue
            value = json.loads(line)
            if not value.get('case'): continue
            binary = value['binary']
            row = totals.setdefault(binary, Counter())
            row['cases'] += 1
            row['captured_views'] += len(value.get('captured_views',[]))
            row['cli_receipts'] += len(value.get('cli_receipts',[]))
            for key in ('physical_posts','summary_posts','file_creates','file_edits','ordinary_effects','effects','Build_effects','Plan_effects','config_instruction_opens'):
                row[key] += value.get(key,0)
            if 'config_instruction_opens' in value: row['discovery_traps'] += 1
            lanes = value.get('dispatches',{})
            for lane,count in (lanes.items() if isinstance(lanes,dict) else lanes):
                row['dispatch_'+lane] += count
            for view in value.get('captured_views',[]):
                row['view_'+view['lane']] += 1
        for binary,row in totals.items(): print(json.dumps({'binary':binary,**row}))
        return
    if names[:1] == ['--text']:
        for name in names[1:]:
            assert name.replace('-','').isalnum()
            print(gzip.decompress((BENCH/('t45-plan-'+name+'.log.gz')).read_bytes()).decode())
        return
    if names in (['--index'], ['--totals']):
        failed_bytes,total,failed_count,receipt_count = 0,0,0,0
        for path in sorted(BENCH.glob('t45-plan-*.log.gz')):
            text = gzip.decompress(path.read_bytes()).decode()
            receipt = json.loads(text.split('\nRECEIPT ')[-1])
            counts = [tuple(map(int,match)) for line in receipt['counts'] for match in re.findall(r'test result: .*? (\d+) passed; (\d+) failed; (\d+) ignored;',line)]
            summary = dict(name=path.name.removeprefix('t45-plan-').removesuffix('.log.gz'),command=receipt['command'],exit=receipt['exit'],source=receipt['source'],passed=sum(row[0] for row in counts),failed=sum(row[1] for row in counts),ignored=sum(row[2] for row in counts),retained_bytes=path.stat().st_size)
            if not counts and len(text)<1500: summary['output'] = text.split('\nRECEIPT ')[0]
            total += path.stat().st_size
            receipt_count += 1
            if receipt['exit']:
                failed_bytes += path.stat().st_size
                failed_count += 1
            if names == ['--index']: print(json.dumps(summary))
        print(json.dumps({'receipt_count':receipt_count,'failed_receipt_count':failed_count,'all_private_gzip_bytes':total,'failed_private_gzip_bytes':failed_bytes}))
        return
    total = 0
    for name in names:
        assert name.replace('-','').isalnum()
        path = BENCH/('t45-plan-'+name+'.log.gz')
        text = gzip.decompress(path.read_bytes()).decode()
        total += path.stat().st_size
        receipt = json.loads(text.split('\nRECEIPT ')[-1])
        print(json.dumps({'name':name,**receipt,'retained_bytes':path.stat().st_size}))
        if receipt['exit']:
            for block in re.findall(r'---- .*? ----\n.*?(?=\nfailures:|\n---- |\ntest result:)',text,re.S):
                print(block)
            if not receipt['counts']:
                print(text[-3000:])
        cases = []
        for line in text.splitlines():
            if not line.startswith('{'): continue
            value = json.loads(line)
            if value.get('case'):
                summary = {key:value[key] for key in ('binary','case','family','custom','constraint','physical_posts','dispatches','summary_posts','effects','file_creates','file_edits','ordinary_effects','Build_effects','Plan_effects','config_instruction_opens','child_requests') if key in value}
                if 'captured_views' in value:
                    summary['captured_views'] = len(value['captured_views'])
                if 'cli_receipts' in value:
                    summary['cli_exits'] = [(row['phase'],row['exit']) for row in value['cli_receipts']]
                cases.append(summary)
            if 'before' in value or 'after' in value: print(json.dumps(value))
        if cases: print(json.dumps({'cases':cases}))
    print(json.dumps({'selected_receipt_gzip_bytes':total}))

if __name__=='__main__': main()
