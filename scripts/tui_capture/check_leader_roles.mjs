#!/usr/bin/env node
// Read-only qualification of real captured semantic roles, not a renderer model.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(process.argv[2]),read=p=>JSON.parse(fs.readFileSync(p));
const result={status:'OBSERVED_ROLE_AND_VISIBLE_HINT_CONTRACTS',full_visual_gate:'NOT_PASS',runs:{}};
const at=(g,text)=>{
  for(let y=0;y<g.rows;y++){const row=g.cells[y],x=row.map(c=>c.symbol).join('').indexOf(text);if(x>=0)return {x,y,cell:row[x]};}
  throw Error('Missing actual visible text: '+text);
};
for(const name of ['paired-default-fresh','paired-nested-fresh','paired-configured-fresh','paired-precedence-fresh','paired-legacy-v1-02-fresh']) {
  const run=result.runs[name]={};
  for(const side of ['upstream','oc']) {
    const grid=n=>read(path.join(root,name,side,'leader-'+n+'.cells.json'));
    const normal=grid('normal'),pending=grid('invalid-pending'),restored=grid('idle-restored');
    const observations=[];
    for(const [stage,g]of [['normal',normal],['pending',pending],['restored',restored]]) {
      const roles=Object.fromEntries(['VIS11','┃','Build','MiMo-V2.6-Flash Free','OpenCode Zen','fast'].map(text=>[text,at(g,text).cell.fg]));
      const expected=stage==='pending'?['#808080','#484848','#484848','#808080','#808080','#f5a742']:['#eeeeee','#5c9cf5','#5c9cf5','#eeeeee','#808080','#f5a742'];
      assert.deepEqual(Object.values(roles),expected);observations.push({stage,roles,cursor:g.cursor});
    }
    const chip=[];
    for(const stage of ['normal','pending','restored']){const g=grid('chip-'+stage),c=at(g,'[Pasted').cell;assert.equal(c.fg,'#0a0a0a');assert.equal(c.bg,'#f5a742');chip.push({stage,fg:c.fg,bg:c.bg,cursor:g.cursor});}
    const commands=grid('modal-normal'),text=commands.cells.map(r=>r.map(c=>c.symbol).join('')).join('\n');
    const prefix=['paired-default-fresh','paired-nested-fresh'].includes(name)?'ctrl+x':'ctrl+g';
    for(const [label,key]of [['Switch model','m'],['Switch session','l'],['New session','n']]) {
      const row=text.split('\n').find(r=>r.includes(label));assert(row?.includes(prefix+' '+key));
    }
    if(prefix==='ctrl+g')assert(!text.includes('ctrl+x'));
    run[side]={roles:observations,chips:chip,visible_commands_prefix:prefix};
  }
}
fs.writeFileSync(path.join(root,'roles-and-hints.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});
console.log('Observed role transitions, unchanged chip RGB and visible effective-leader hints verified in all five captured modes. Full visual gate remains NOT_PASS.');
