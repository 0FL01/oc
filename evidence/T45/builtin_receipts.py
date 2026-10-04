#!/usr/bin/env python3
"""Summarize only this atomic's immutable receipts and retained ELF proof."""
import gzip
import json
from pathlib import Path
import re
import sys

BENCH = Path('/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924')

def main():
    for name in sys.argv[1:]:
        if name=='--inventory':
            logs=list(BENCH.glob('t45-builtin-*.log.gz'))
            evidence=[Path(__file__).parent/file for file in ('builtin-profiles.md','builtin_receipts.py',
                'native_builtin_profiles.py','native_builtin_dcp_calculator.py','run_builtin_check.py')]
            rows=[]
            removed=[]
            for log in sorted(logs):
                text=gzip.open(log,'rt').read()
                receipt=json.loads(text.rsplit('\nRECEIPT ',1)[1])
                rows.append({'log':log.name,'bytes':log.stat().st_size,'exit':receipt['exit'],
                             'source':receipt['source'],'code_diff':receipt.get('code_diff')})
                for line in text.splitlines():
                    if not line.startswith('{'):
                        continue
                    row=json.loads(line)
                    if 'owner_cleanup' in row:
                        assert row['all_joined'] and row['exact_temp_removed'] and not Path(row['owner_cleanup']).exists()
                        removed.append(row['owner_cleanup'])
            total=sum(log.stat().st_size for log in logs)+sum(file.stat().st_size for file in evidence)
            remaining=[str(path) for path in BENCH.glob('t45-builtin-*') if path.is_dir()]
            assert total<=1024*1024 and not remaining
            print(json.dumps({'immutable_log_directory':str(BENCH),'logs':rows,'total_atomic_evidence_log_bytes':total,
                              'verified_cleanup_receipts':len(removed),'remaining_builtin_fixture_dirs':remaining}))
            continue
        path = BENCH/('t45-builtin-'+name+'.log.gz')
        text = gzip.open(path,'rt').read()
        receipt = json.loads(text.rsplit('\nRECEIPT ',1)[1])
        counts = re.findall(r'test result: .*? (\d+) passed; (\d+) failed; (\d+) ignored;',text.split('\nRECEIPT ',1)[0])
        totals = [sum(int(row[index]) for row in counts) for index in range(3)]
        print(json.dumps({'log':str(path),'receipt':{key:value for key,value in receipt.items() if key!='counts'},
                          'passed_failed_ignored':totals,'python_tests':re.findall(r'Ran (\d+) tests',text)}))
        if receipt['exit']:
            for block in text.split('failures:')[1:]:
                if '\n---- ' in block:
                    print(block.split('test result:',1)[0])
        native = []
        cases=[]
        for line in text.splitlines():
            if line.startswith('{'):
                row = json.loads(line)
                if 'before' in row or 'after' in row or 'sha256_before' in row:
                    print(json.dumps(row))
                if 'binary' in row and 'requests' in row:
                    native.append(row)
                if row.get('status')=='PASS':
                    cases.append(row)
        for binary in sorted({row['binary'] for row in native}):
            rows = [row for row in native if row['binary']==binary]
            if binary==min(row['binary'] for row in native):
                for row in rows:
                    print(json.dumps({key:row[key] for key in ('primary','family','constraint','requests',
                        'tool_intents','http_effects','stdio_effects','native_file_effects')}))
            print(json.dumps({'binary':binary,'cases':len(rows),
                              'requests':{lane:sum(row['requests'][lane] for row in rows) for lane in ('main','general','explore','title')},
                              'tool_intents':sum(row['tool_intents'] for row in rows),
                              'http_effects':sum(sum(row['http_effects'].values()) for row in rows),
                              'stdio_effects':sum(sum(row['stdio_effects'].values()) for row in rows),
                              'native_file_effects':sum(row['native_file_effects'] for row in rows),
                              'captured_views_checked':sum(row['captured_views_checked'] for row in rows)}))
        if cases:
            numeric={key:sum(row.get(key,0) for row in cases) for key in
                     ('physical_posts','main_posts','auxiliary_posts','primary_requests','summaries','auxiliary_requests','tool_rows')
                     if any(key in row for row in cases)}
            print(json.dumps({'cases':len(cases),'actual_counters':numeric,
                              'sql_tool_rows':sum(len(row.get('sql',[])) for row in cases),
                              'sql_completed_compress':sum(sum(tool=='compress' and state=='completed' for tool,state in row.get('sql',[])) for row in cases)}))

if __name__=='__main__':
    main()
