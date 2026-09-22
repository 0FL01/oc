#!/usr/bin/env node
// Targeted observations from independent actual VT grids, never source goldens.
import fs from 'node:fs';
import path from 'node:path';
const root = process.argv[2];
const output = process.argv[3];
function observe(attempt, origin, scenario) {
  const file = path.join(root, attempt, origin, scenario+'.cells.json');
  const g = JSON.parse(fs.readFileSync(file));
  const lines = g.cells.map(row=>row.map(c=>c.symbol).join(''));
  const blank = g.cells[10];
  const bg = blank.at(-1).bg;
  let start = blank.length;
  while(start>0 && blank[start-1].bg===bg) start--;
  const sidebar = start>0 && bg!==blank[0].bg ? {x:start,width:blank.length-start,bg} : null;
  return {columns:g.columns,rows:g.rows,sidebar,
    visible_row_markers:(lines.join('\n').match(/ROW-\d+/g)||[]).length,
    short_answer_y:lines.findIndex(l=>l.includes('GEOMETRY-SHORT')),
    user_prompt_y:lines.findIndex(l=>l.includes('Какие тебе тулы доступны?')),
    underline_y:lines.findIndex(l=>l.includes('╹')),
    context_y:lines.findIndex(l=>l.includes('Context')),
    usage_y:lines.findIndex(l=>l.includes('6,763 tokens')),
    footer_y:lines.findIndex(l=>l.includes('ctrl+p commands'))};
}
const pairs=[];
for(const [attempt,scenarios] of [
  ['v03-attempt-05',['session-wide-completed','resize-80x24-0','resize-120x40-1','resize-160x48-2','resize-43x48-3','resize-44x48-4','resize-119x48-5','resize-120x48-6','resize-121x48-7','resize-120x80-8','resize-160x48-9']],
  ['v03-attempt-06',['session-wide-completed']],
  ['v03-attempt-07',['session-wide-completed']],
  ['v03-attempt-08',['session-wide-completed','resize-80x24-0','resize-120x40-1','resize-160x48-2','resize-43x48-3','resize-44x48-4','resize-119x48-5','resize-120x48-6','resize-121x48-7','resize-120x80-8','resize-160x48-9']],
]) {
  for(const scenario of scenarios) {
    const reference=observe(attempt,'upstream',scenario), actual=observe(attempt,'oc',scenario);
    const fields=['columns','rows','sidebar','short_answer_y','user_prompt_y','underline_y','context_y','usage_y','footer_y'];
    pairs.push({attempt,scenario,reference,actual,
      equal_geometry_fields:Object.fromEntries(fields.map(k=>[k,JSON.stringify(reference[k])===JSON.stringify(actual[k])]))});
  }
}
const summary={scope:'Targeted geometry observations only. Full grid/PNG diffs remain DIFFERENT; no VIS or T44 PASS.',pairs};
fs.writeFileSync(output,JSON.stringify(summary,null,2)+'\n');
console.log(JSON.stringify({output,pairs:pairs.length,mismatches:pairs.filter(p=>Object.values(p.equal_geometry_fields).some(v=>!v)).map(p=>({attempt:p.attempt,scenario:p.scenario,fields:p.equal_geometry_fields})),scope:summary.scope},null,2));
