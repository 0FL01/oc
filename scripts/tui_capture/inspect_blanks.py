"""Inspect actual cell attributes; never normalize/mask comparator inputs."""
import collections
import json
import sys
from pathlib import Path

for argument in sys.argv[1:]:
    path = Path(argument)
    capture = json.loads(path.read_text())
    blanks = collections.Counter(
        (cell["fg"], tuple(cell["modifiers"]))
        for row in capture["cells"]
        for cell in row
        if cell["symbol"] == " "
    )
    print(path, "blank (fg, modifiers) counts:", blanks)
    if "--detail" not in sys.argv:
        continue
    print("nondefault blanks:", [
        (x, y, cell["fg"], cell["modifiers"])
        for y, row in enumerate(capture["cells"])
        for x, cell in enumerate(row)
        if cell["symbol"] == " " and (cell["fg"] != "#ffffff" or cell["modifiers"])
    ])
