#!/usr/bin/env python3
"""Read-only analysis of the opt-in isolated subprocess strace evidence."""
import json
from pathlib import Path
import re
import sys

capture, output = Path(sys.argv[1]), Path(sys.argv[2])
if output.exists(): raise RuntimeError('Immutable output exists')
protocol = json.loads((capture/'oc/protocol.json').read_text())
records = []
for generation in (0,1):
    source = capture/'oc'/f'permission.strace.generation{generation}'
    for number,line in enumerate(source.read_text().splitlines(),1):
        match = re.match(r'\d+\s+(\d+\.\d+)\s+(.*)',line)
        if match:
            records.append({'file':str(source),'line':number,'time_ns':int(float(match[1])*1e9),'text':match[2]})
network = [r for r in records if re.search(r'\b(connect|sendto|sendmsg|sendmmsg)\(',r['text'])]
destinations = [r for r in network if 'sa_family=AF_INET' in r['text']]
unexpected = [r for r in destinations if not ('inet_addr("127.0.0.1")' in r['text'] or 'inet_pton(AF_INET6, "::1"' in r['text'])]
dns = [r for r in records if re.search(r'(sin6?_port=htons\(53\)|/etc/(resolv.conf|nsswitch.conf)|nscd/socket|systemd/resolve)',r['text'])]
windows = []
for case in ['once','reject','always','mcp','childroot','read','shell','glob','url']:
    provider_case = 'childread' if case=='childroot' else case
    request = next(e for e in protocol if e['kind']=='provider' and e['case']==provider_case and e.get('operation')!='title')
    held = next(e for e in protocol if e['kind']=='permission_snapshot' and e['request_id']==case+'-held')
    begin,end = request['wall_time_ns'],held['wall_time_ns']
    observed = [r for r in records if begin<=r['time_ns']<=end]
    target = [r for r in observed if re.search(r'"(?:[^"\n]*/)?(?:approval.txt|denied.txt|shell-marker)"',r['text'])]
    opens = [r for r in target if re.search(r'\bopen(at|at2)?\(',r['text'])]
    writes = [r for r in target if re.search(r'(O_WRONLY|O_RDWR|O_TRUNC|O_CREAT|\b(rename\w*|unlink\w*|chmod|fchmodat|truncate|mkdir\w*|link\w*|symlink\w*)\()',r['text'])]
    windows.append({'case':case,'from_provider_observation_ns':begin,'through_held_snapshot_ns':end,
        'target_opens':opens,'target_mutation_calls':writes,'network_destinations':[r for r in destinations if begin<=r['time_ns']<=end],
        'no_target_write_open_or_path_mutation':not writes})
report = {'capture':str(capture),'scope':'Only actual native local-fixture subprocess and descendants; strace -f -ttt -yy -e trace=%file,%network. No attach to authoring processes. Held windows include actual provider dispatch and preview preparation.',
    'network_destination_calls':destinations,'nonloopback_destinations':unexpected,'dns_transport_or_resolver_setup_calls':dns,
    'windows':windows,'pass':not unexpected and not dns and all(w['no_target_write_open_or_path_mutation'] for w in windows),
    'limits':['File/network syscall selection does not trace read/write payloads, mmap dirty pages or kernel-internal cache lookups.',
              'Absence of transport/resolver setup calls is a bounded DNS/network observation, not a universal guarantee or typed CoreEvent evidence.',
              'Read-only file snapshots independently check bytes/mode/mtime; storage WAL writes are expected and outside the target-file mutation assertion.']}
output.write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'output':str(output),'pass':report['pass'],'destination_calls':len(destinations),
    'nonloopback_calls':len(unexpected),'dns_calls':len(dns),'windows':[{'case':w['case'],'opens':len(w['target_opens']),'mutation_calls':len(w['target_mutation_calls'])} for w in windows]},indent=2))
sys.exit(0 if report['pass'] else 1)
