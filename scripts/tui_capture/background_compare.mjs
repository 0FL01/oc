// Compare observed complete components in sealed full frames; never synthesize
// a reference image, rewrite a cell, or mask the independent whole-frame report.
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {backgroundComponent,backgroundCanonical} from './background_subagents.mjs';
const descriptions={A:'Сравнение subagent с upstream',B:'Состояние Git и тесты'};
const glyphs=row=>row.map(cell=>cell.width===0?'':cell.symbol).join('');
const requireThat=(condition,message)=>{if(!condition)throw Error(message);};

export function backgroundRegions(frame,stage) {
  const cards=Object.fromEntries(['A','B'].map(key=>[key,backgroundComponent(frame,key)]));
  requireThat(cards.A.y<cards.B.y,'Original launch order changed');
  const keys=stage==='running-A'?['A']:stage==='running-B'?['B']:['A','B'];
  const regions=keys.map(key=>({name:'card-'+key,y:cards[key].y,cells:cards[key].rows}));
  const done=stage==='B-complete-A-running'?['B']:['completed','reopened'].includes(stage)?['B','A']:[];
  for(const key of ['A','B'])requireThat((cards[key].glyph==='✓')===done.includes(key),'Wrong actual child activity '+key);
  const text=frame.cells.map(glyphs);
  const parent=text.findIndex(row=>row.includes('PARENT-STEP'));
  requireThat(parent>=0,'Parent response anchor must be visible');
  let previous=-1;
  for(const key of done) {
    const matches=text.flatMap((row,y)=>row.includes('↳ Explore finished · '+descriptions[key])?[y]:[]);
    requireThat(matches.length===1,'Exactly one whole notice required '+key);
    const y=matches[0];requireThat(y>parent&&y>previous,'Notice chronology relative to parent/peer');previous=y;
    requireThat(y>0&&text[y-1].trim()==='','Notice leading margin missing');
    regions.push({name:'notice-'+key,y:y-1,cells:frame.cells.slice(y-1,y+1),parent_offset:y-parent});
  }
  return regions;
}

export function compareBackgroundCells(reference,actual,stage) {
  const a=backgroundRegions(reference,stage),b=backgroundRegions(actual,stage);
  requireThat(a.length===b.length,'Component inventory mismatch');
  return Object.fromEntries(a.map((left,i)=>{
    const right=b[i];requireThat(left.name===right.name,'Component identity mismatch');
    return [left.name,{styled_cells_equal:backgroundCanonical(left.cells)===backgroundCanonical(right.cells),
      relative_anchor_equal:left.parent_offset===right.parent_offset,left:{y:left.y,rows:left.cells.length,columns:left.cells[0].length},
      right:{y:right.y,rows:right.cells.length,columns:right.cells[0].length}}];
  }));
}

export function compareBackground(output,behavior,env) {
  const report={status:'NOT_QUALIFIED',scope:'Complete original launch cards including wrapped Background, notices and relative parent anchors; independent observed running phases only',states:{},whole_frame:'Independent full unmasked grids/PNGs/cursors retained; no whole-scene pixel PASS'};
  try {
    requireThat(['upstream','oc'].every(origin=>behavior[origin]?.composer?.status==='PASS_BEHAVIOR_ONLY'),'Both actual lifecycle behavior probes must pass');
    const pixels=[];
    for(const stage of ['running-A','running-B','B-complete-A-running','completed','reopened']) {
      const stem=origin=>path.join(output,origin,'tool-preview-background-'+stage);
      const frames=['upstream','oc'].map(origin=>JSON.parse(fs.readFileSync(stem(origin)+'.cells.json','utf8')));
      const regions=compareBackgroundCells(...frames,stage);
      report.states[stage]=regions;
      for(const [name,region] of Object.entries(regions))pixels.push({id:stage+'/'+name,
        left:{stem:stem('upstream'),...region.left},right:{stem:stem('oc'),...region.right}});
    }
    const result=spawnSync('python3',['-c',`
import hashlib,json,sys
from PIL import Image
output={}
for item in json.loads(sys.argv[1]):
    regions=[]; geometry=[]
    for side in ('left','right'):
        source=item[side]
        with open(source['stem']+'.render.json') as file: metric=json.load(file)['before']
        cw,ch=metric['measured_cell_width'],metric['measured_cell_height']
        if metric['device_pixel_ratio']!=1: raise ValueError('unqualified raster device scale')
        geometry.append((cw,ch,source['rows'],source['columns']))
        with Image.open(source['stem']+'.png') as image:
            region=image.convert('RGBA').crop((0,round(source['y']*ch),round(source['columns']*cw),round((source['y']+source['rows'])*ch)))
            if region.getcolors(1) is not None: raise ValueError('no actual component glyph pixels')
            regions.append(region.tobytes())
    output[item['id']]={'pixels_equal':geometry[0]==geometry[1] and regions[0]==regions[1],
        'sha256':[hashlib.sha256(region).hexdigest() for region in regions]}
print(json.dumps(output))
`,JSON.stringify(pixels)],{encoding:'utf8',env});
    requireThat(result.status===0,result.stderr||result.error?.message||'Raster comparison failed');
    const rasters=JSON.parse(result.stdout);
    for(const [stage,regions] of Object.entries(report.states))for(const [name,region] of Object.entries(regions))Object.assign(region,rasters[stage+'/'+name]);
    report.status=Object.values(report.states).every(regions=>Object.values(regions).every(region=>region.styled_cells_equal&&region.relative_anchor_equal&&region.pixels_equal))?'TARGETED_COMPONENT_PASS':'DIFFERENT';
  } catch(error) {report.error=error.stack;}
  return report;
}
