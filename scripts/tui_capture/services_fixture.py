"""VIS42 owned local plugin/MCP failures; neither renderer nor provider is replaced."""
import json
import os
from pathlib import Path
import sys
import time


def configure(spec, home, project, config, cli):
    plugin = home / 'vis42-plugin'
    plugin.mkdir()
    (plugin / 'package.json').write_text(json.dumps({'type': 'module', 'main': 'index.js'}))
    (plugin / 'index.js').write_text('throw new Error("VIS42_BACKGROUND_CANARY"); export default {};\n')
    config['plugins' if spec['origin'] == 'upstream' else 'plugin'] = (
        ['-opencode.models.dev', str(plugin)] if spec['origin'] == 'upstream'
        else ['@tarquinen/opencode-dcp', str(plugin)])
    config['compaction'] = {'auto': False, 'prune': True}
    command = ['/usr/bin/python3', str(Path(__file__).resolve()), 'peer', str(home)]
    server = {'type': 'local', 'command': command,
              'timeout': {'startup': 10000, 'catalog': 10000, 'execution': 10000}}
    config['mcp'] = ({'servers': {'vis42': {**server, 'codemode': False}}}
                     if spec['origin'] == 'upstream' else {'vis42': {**server, 'enabled': True}})
    (home / 'vis42-phase.json').write_text(json.dumps({'phase': 'held'}))
    cli['session']['tps'] = False


def control(home, config, origin, action):
    if action not in ('fail', 'recover'):
        raise ValueError('Unknown bounded VIS42 fixture action')
    phase = 'failed' if action == 'fail' else 'healthy'
    temporary = home / 'vis42-phase.next'
    temporary.write_text(json.dumps({'phase': phase}))
    temporary.replace(home / 'vis42-phase.json')
    if action == 'recover':
        config['plugins' if origin == 'upstream' else 'plugin'] = (
            ['-opencode.models.dev'] if origin == 'upstream' else ['@tarquinen/opencode-dcp'])
        config.pop('compaction', None)
        (home / 'config/opencode/opencode.json').write_text(json.dumps(config))
    return {'action': action, 'phase': phase}


def snapshot(home):
    log = home / 'vis42-mcp.jsonl'
    rows = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
    return {'mcp': rows, 'phase': json.loads((home / 'vis42-phase.json').read_text())['phase']}


def peer(home):
    for line in sys.stdin:
        request = json.loads(line)
        method = request.get('method')
        if method == 'initialize':
            with (home / 'vis42-mcp.jsonl').open('a') as log:
                log.write(json.dumps({'pid': os.getpid(), 'method': method,
                                      'phase': 'admitted-before-release',
                                      'at_ms': int(time.time() * 1000)}) + '\n')
            deadline = time.monotonic() + 10
            while json.loads((home / 'vis42-phase.json').read_text())['phase'] == 'held':
                if time.monotonic() >= deadline:
                    raise TimeoutError('Bounded VIS42 fixture release was not received')
                time.sleep(.01)
        phase = json.loads((home / 'vis42-phase.json').read_text())['phase']
        with (home / 'vis42-mcp.jsonl').open('a') as log:
            log.write(json.dumps({'pid': os.getpid(), 'method': method, 'phase': phase,
                                  'at_ms': int(time.time() * 1000)}) + '\n')
        if 'id' not in request:
            continue
        if method == 'initialize' and phase == 'failed':
            response = {'error': {'code': -32603, 'message': 'VIS42_BACKGROUND_CANARY'}}
        else:
            result = ({'protocolVersion': request['params']['protocolVersion'], 'capabilities': {},
                       'serverInfo': {'name': 'vis42-local', 'version': '1'}}
                      if method == 'initialize' else {'tools': []} if method == 'tools/list' else {})
            response = {'result': result}
        print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], **response}), flush=True)


if __name__ == '__main__':
    if len(sys.argv) != 3 or sys.argv[1] != 'peer':
        raise SystemExit('Only the owned VIS42 peer invocation is accepted')
    peer(Path(sys.argv[2]))
