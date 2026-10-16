"""Read-only qualification of actual runner-produced styled modal cells."""
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
for origin in ('upstream', 'oc'):
    base = json.loads((root / origin / 'session-wide-completed.cells.json').read_text())['cells']
    for scenario in ('commands-over-session', 'models-over-session'):
        grid = json.loads((root / origin / f'{scenario}.cells.json').read_text())['cells']
        print(origin, scenario)
        for x,y in [(50,12),(54,13),(54,15),(54,17),(54,18),(54,19),(52,20),(103,18),(0,0),(120,5)]:
            print(f'  ({x},{y})', grid[y][x])
        floor_errors = round_errors = fields = 0
        for y in range(len(grid)):
            for x in range(len(grid[y])):
                if 50 <= x < 110 and 12 <= y < 38:
                    continue
                a,b = base[y][x],grid[y][x]
                if a['symbol'] != b['symbol']:
                    continue
                for key in ('fg','bg'):
                    if key == 'fg' and a['symbol'].isspace():
                        continue # upstream leaves default blank-cell fg untouched
                    if not all(isinstance(c,str) and len(c)==7 and c.startswith('#') for c in (a[key],b[key])):
                        continue
                    rgb = [int(a[key][i:i+2],16) for i in (1,3,5)]
                    floor = '#' + ''.join(f'{v*105//255:02x}' for v in rgb)
                    rounded = '#' + ''.join(f'{(v*105+127)//255:02x}' for v in rgb)
                    floor_errors += b[key] != floor
                    round_errors += b[key] != rounded
                    fields += 1
        print('  underlay RGB fields',fields,'floor mismatches',floor_errors,'nearest mismatches',round_errors)
