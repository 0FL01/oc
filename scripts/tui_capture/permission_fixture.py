"""VIS36 actual ordinary patch executor, read-only permission/effect audit."""
import hashlib
import json
import sqlite3
import apply_patch_fixture

FILES = ['approval.txt', 'denied.txt', 'shell-marker']
LONG = 'new approved ' + 'fixture wrap words ' * 12
PATCHES = {
    'once': '*** Update File: approval.txt\n@@\n-before approval\n+' + LONG,
    'reject': '*** Update File: approval.txt\n@@\n-' + LONG + '\n+rejected must never appear',
    'always': '*** Update File: approval.txt\n@@\n-' + LONG + '\n+always approved',
    'restart': '*** Update File: approval.txt\n@@\n-always approved\n+restart approved',
    'mixed': '*** Update File: approval.txt\n@@\n-restart approved\n+mixed must never appear\n*** Update File: denied.txt\n@@\n-denied sentinel\n+deny must never appear',
}

def configure(spec, home, project, config, cli):
    apply_patch_fixture.configure(spec, home, project, config, cli)
    (project / 'approval.txt').write_text('before approval\n')
    (project / 'approval.txt').chmod(0o640)
    spec['permission_patches'] = PATCHES
    spec['permission_calls'] = {
        'read': {'name':'read','arguments':{'path':'approval.txt','offset':1,'limit':5}},
        'shell': {'name':'bash' if spec['origin']=='oc' else 'shell','arguments':{'argv':['/usr/bin/touch','shell-marker']} if spec['origin']=='oc' else {'command':'/usr/bin/touch shell-marker'}},
        'glob': {'name':'glob','arguments':{'pattern':'**/*.txt'}},
        'url': {'name':'webfetch','arguments':{'url':'https://example.invalid/vis36/never-fetch?wrapped='+'word-'*24}},
        'autoonce': {'name':'read','arguments':{'path':'approval.txt','limit':5}},
        'promptagain': {'name':'read','arguments':{'path':'approval.txt','limit':5}},
        'mcp': {'name':'vis36__echo' if spec['origin']=='oc' else 'vis36_echo','arguments':{'message':'bounded real call ' + 'wrap words ' * 12}},
        'childroot': {'name':'subagent','arguments':{'agent':'vis36-child','description':'VIS36 child permission feedback','prompt':'VIS36 childread: read approval.txt; honor rejection feedback.'}},
        'childread': {'name':'read','arguments':{'path':'approval.txt','limit':5}},
    }
    spec['permission_patches'] = {**PATCHES, **{key:'' for key in spec['permission_calls']}}
    marker = project / 'shell-marker'
    if marker.exists(): marker.unlink()
    if spec['origin'] == 'upstream':
        config['permissions'] = [{'action':'*','resource':'*','effect':'ask'}, {'action':'edit','resource':'denied.txt','effect':'deny'}, {'action':'subagent','resource':'*','effect':'allow'}]
        config['agents'] = {'vis36-child':{'mode':'subagent','system':'Isolated VIS36 child; follow the supplied task.'}}
        config['mcp'] = {'servers':{'vis36':{'type':'local','command':['/usr/bin/python3',str(home/'vis36-mcp.py')],'codemode':False}}}
    else:
        config['permissions'] = {'*':'ask','subagent':'allow','apply_patch':{'*':'deny','approval.txt':'ask','denied.txt':'deny'}}
        config['agent'] = {'vis36-child':{'mode':'subagent','prompt':'Isolated VIS36 child; follow the supplied task.'}}
        config['mcp'] = {'vis36':{'type':'local','command':['/usr/bin/python3',str(home/'vis36-mcp.py')],'enabled':True}}
    (home/'vis36-mcp.py').write_text((__import__('pathlib').Path(__file__).parent/'permission_mcp.py').read_text())
    cli['session']['permissions'] = 'autoaccept' if spec.get('permission_mode') == 'auto-config' else 'prompt'
    if spec.get('permission_mode') in ('auto-config','auto-cli'):
        spec['permission_patches']['mixed'] = PATCHES['mixed'].replace('-restart approved', '-' + LONG)

def snapshot(home, project):
    value = apply_patch_fixture.snapshot(home, project)
    value['files'] = {}
    for name in FILES:
        file = project / name
        if not file.exists():
            value['files'][name] = {'present':False}
            continue
        stat = file.stat()
        data = file.read_bytes()
        value['files'][name] = {'present':True,'bytes_hex':data.hex(),'sha256':hashlib.sha256(data).hexdigest(),'mode':oct(stat.st_mode & 0o777),'mtime_ns':stat.st_mtime_ns}
    for observation in value['observations']:
        with sqlite3.connect(__import__('pathlib').Path(observation['database']).as_uri()+'?mode=ro', uri=True) as connection:
            connection.row_factory = sqlite3.Row
            for table in observation['tables']:
                if 'permission' in table or table == 'events':
                    observation['data'][table] = [dict(row) for row in connection.execute(f'SELECT * FROM "{table}" LIMIT 200')]
    cli = home / 'config/opencode/cli.json'
    value['cli'] = json.loads(cli.read_text()) if cli.exists() else None
    value['audit'] = 'File stat/hash/bytes and SQLite mode=ro; not an OS syscall trace.'
    log = home/'vis36-mcp.jsonl'
    value['mcp'] = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
    return value

def respond(handler, body, spec, emit):
    return apply_patch_fixture.respond(handler, body, spec, emit)
