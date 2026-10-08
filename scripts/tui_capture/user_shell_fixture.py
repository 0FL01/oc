"""Actual user command effect; native SQL is observed before the first effect."""
import json

COMMAND = 'python3 user-shell-probe.py'


def configure(spec, home, project, config):
    if spec['origin'] == 'upstream':
        config['permissions'] = [{'action':'*','resource':'*','effect':'deny'},
                                 {'action':'shell','resource':COMMAND,'effect':'allow'}]
    else:
        config['permissions'] = {'*':'deny', 'bash':{'*':'deny', COMMAND:'allow'}}
    # No alternate executor: the real composer must launch this ordinary file
    # with the original session.shell / native supervised Jobs owner.
    (project / 'user-shell-probe.py').write_text(f'''
import json, pathlib, sqlite3
command = {COMMAND!r}
origin = {spec['origin']!r}
fact = {{'origin':origin}}
if origin == 'oc':
    database = pathlib.Path({str(home / 'data/oc/oc.sqlite')!r})
    with sqlite3.connect(database.as_uri() + '?mode=ro', uri=True) as connection:
        history = json.loads(connection.execute("SELECT value FROM prefs WHERE key='tui.prompt_history.v1'").fetchone()[0])
        operation, session, turn, state, args = connection.execute("SELECT id,session_id,turn_id,state,input FROM tool_operations WHERE name='shell' ORDER BY rowid DESC LIMIT 1").fetchone()
        turns = connection.execute('SELECT COUNT(*) FROM turns').fetchone()[0]
        assert history == [command] and turn is None and state == 'started' and turns == 0
        assert json.loads(args)['command'] == command
        fact.update(operation=operation,session=session,turn=turn,state=state,model_turns=turns,history_exact=True)
with pathlib.Path('user-shell-' + origin + '-boundary.jsonl').open('a') as log:
    log.write(json.dumps(fact) + '\\n')
with pathlib.Path('tool-preview-' + origin + '.effects').open('a') as effect:
    effect.write('VIS-USER-SHELL-EFFECT\\n')
print('VIS-USER-SHELL-DONE', flush=True)
''')


def boundary(project, spec):
    file = project / ('user-shell-' + spec['origin'] + '-boundary.jsonl')
    if not file.exists():
        return []
    with file.open() as log:
        data = log.read(4097)
    if len(data) > 4096:
        raise AssertionError('User Shell boundary exceeded bounded fixture size')
    return [json.loads(line) for line in data.splitlines()]
