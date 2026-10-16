"""Read-only r3: unchanged r2/r1 codec checks plus foreign progress-field seals."""
import ast
from pathlib import Path
import stat
SOURCE=Path(__file__).with_name('vis36-vis38-durable-check-r1.py')
tree=ast.parse(SOURCE.read_bytes(),filename=str(SOURCE))
tree.body=[n for n in tree.body if not isinstance(n,ast.If)]
exec(compile(ast.fix_missing_locations(tree),str(SOURCE),'exec'))

def progress_seals():
    old=check.load(audit.BEFORE)['state'];new=check.load(REPO/'progress/STATE.json')
    assert new['current']==old['current']=='T44'
    assert new['last_checkpoint_task']==old['last_checkpoint_task']=='T44'
    assert new['schema_version']==old['schema_version']
    assert set(new['tasks'])==set(old['tasks'])
    for task,previous in old['tasks'].items():
        current=new['tasks'][task]
        if task!='T44':assert current==previous,task
        else:
            assert set(current)==set(previous)
            assert all(current[k]==v for k,v in previous.items() if k not in ('last_seq','latest'))
            assert current['last_seq'] in (142,143)

if __name__=='__main__':
    progress_seals();main()
