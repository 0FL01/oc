"""Read-only r2: preserve failed r1 bytes; supply inventory's stat dependency."""
import ast
from pathlib import Path
import stat
SOURCE=Path(__file__).with_name('vis36-vis38-durable-check-r1.py')
tree=ast.parse(SOURCE.read_bytes(),filename=str(SOURCE))
tree.body=[n for n in tree.body if not isinstance(n,ast.If)]
exec(compile(ast.fix_missing_locations(tree),str(SOURCE),'exec'))
if __name__=='__main__':main()
