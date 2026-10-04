#!/usr/bin/env python3
"""Normal ELF calculator regression under the current source-selected cadence.

dcp_auto.rs evaluate_request: context-limit cooldown differs from one advisory
last-user/assistant reminder per new user. Preserve the older helper and its
lossless failure; do not change thresholds or hide a repeated reminder.
"""
import hashlib
import json
from pathlib import Path
import sys

import native_dcp_defaults as defaults


def advisory(binary):
    def respond(_owner, number, request):
        assert request.get('tools'), 'unexpected hidden summary request'
        return defaults.t50.completed('x'*96000 if number==1 else 'closed')
    with defaults.Defaults(binary, respond, {'compress':'allow'}) as native:
        path=native.home/'config/opencode/opencode.json'
        config=json.loads(path.read_text())
        config['provider']['fixture']['models']={'m':{'limit':{'context':40000,'output':2048}}}
        config['dcp']={'compress':{'minContextLimit':100,'maxContextLimit':50000,'summaryBuffer':True}}
        path.write_text(json.dumps(config))
        native.start()
        native.send(b'prime\r')
        defaults.t50.until(lambda:len(native.requests)==1 and native.settled(1),'prime did not close')
        native.send(b'continue\r')
        defaults.t50.until(lambda:len(native.requests)==2 and native.settled(2),'second request did not close')
        fragments=['reminders min 100','max 50K','summary buffer true','model fixture/m']
        before=native.panel(fragments)
        native.stop(); native.start()
        assert native.panel(fragments)==before
        native.send(b'after restart\r')
        defaults.t50.until(lambda:len(native.requests)==3 and native.settled(3),'restart did not close')
        cadence=[len(defaults.reminder(request)) for request in native.requests]
        assert cadence==[0,1,1], cadence
        for request in native.requests[1:]:
            reminder=defaults.reminder(request)[0]
            text=json.dumps(reminder)
            assert 'advisory' in text and 'last-user/assistant reminder' in text
            assert 'reached reminder limit 100' in text and 'context-limit' not in text
        facts=defaults.identity(native)
        assert (facts['context_limit'],facts['dcp_min_context'],facts['dcp_max_context'])==(40000,100,50000)
        assert facts['model']=={'id':'m','provider':'fixture','variant':None}, facts['model']
        assert facts['input_limit']!=50000
        assert native.rows('SELECT count(*) FROM tool_operations')==[(0,)]
        assert native.rows('SELECT count(*) FROM compression_blocks')==[(0,)]
        lanes=dict(native.rows("SELECT json_extract(payload,'$.lane'),count(*) FROM events WHERE kind='generation_dispatched' GROUP BY 1"))
        assert lanes=={'main':3,'title':1} and native.physical_requests==4
        assert native.auxiliary_requests==1 and not native.errors
        print(json.dumps({'case':'explicit-numeric-buffer-current-cadence','status':'PASS',
                          'physical_posts':4,'main_posts':3,'auxiliary_posts':1,'hidden_summary_posts':0,
                          'compress_operations':0,'cadence_wire':cadence,'calculator':[40000,100,50000]}),flush=True)


def main():
    binary=Path(sys.argv[1]).resolve()
    before=hashlib.sha256(binary.read_bytes()).hexdigest()
    for case,limits in [('no-file-known',{'context':40000,'output':2048}),
                        ('no-file-missing',{}),('no-file-zero',{'context':0,'output':0}),
                        ('context-only-warning',{'context':40000})]:
        defaults.qualify(binary,case,limits)
    advisory(binary)
    defaults.qualify(binary,'partial-numeric',{'context':40000,'output':2048},
                     {'minContextLimit':100},thresholds=(100,22000))
    defaults.qualify(binary,'exact-provider-nested-model',{'context':40000,'output':2048},
                     {'modelMinLimits':{'fixture/nested/m':'25%'},'modelMaxLimits':{'fixture/nested/m':'60%'}},
                     model='nested/m',thresholds=(10000,24000))
    defaults.invalid(binary)
    after=hashlib.sha256(binary.read_bytes()).hexdigest()
    assert before==after
    print(json.dumps({'binary':str(binary),'sha256_before':before,'sha256_after':after,
                      'cases':8,'physical_posts':28,'main_posts':21,'auxiliary_posts':7,
                      'hidden_summary_posts':0,'owned_cleanup':'all reused owners joined before exact TempDir cleanup'}),flush=True)

if __name__=='__main__':
    main()
