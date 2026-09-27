#!/usr/bin/env node
// Diagnostic exact-cell audit of current fixes, separate from full comparators.
import fs from 'node:fs';
import path from 'node:path';
const base=process.argv[2],output=process.argv[3];
const first=Number(process.argv[4]||29),last=Number(process.argv[5]||32);
if(fs.existsSync(output))throw Error('Immutable output exists');
const report={scope:`Actual unmasked frames ${first}–${last}; targeted diagnostic cells do not qualify full-grid parity.`,runs:[]};
const find=(f,t)=>{const out=[];for(let y=0;y<f.rows;y++)for(let x=0;x<=f.columns-t.length;x++)if([...t].every((s,i)=>f.cells[y][x+i].symbol===s))out.push({x,y});return out;};
const line=(f,y)=>f.cells[y].map(c=>c.symbol).join('');
for(let n=first;n<=last;n++){
 const name=`permission20260927-${n}`,sides={};
 for(const origin of ['upstream','oc']){
  const frame=s=>JSON.parse(fs.readFileSync(path.join(base,name,origin,`permission-${s}.cells.json`)));
  const ask=frame('once-ask'),full=frame('once-fullscreen'),failed=frame('reject-completed'),feedback=frame('child-feedback-draft'),settings=frame('settings-autoaccept'),url=frame('url-ask');
  const fail=find(failed,'# Patch failed')[0],urlTitle=find(url,'Permission required')[0],urlLabel=find(url,'URL:')[0];
   sides[origin]={columns:ask.columns,patching:find(ask,'Patching'),patching_filename:find(ask,'approval.txt').filter(p=>p.y===find(ask,'Patching')[0].y),
    failed_title:{position:fail,count:find(failed,'# Patch failed').length,line:line(failed,fail.y),filename_on_title:line(failed,fail.y).includes('approval.txt'),filename_positions:find(failed,'approval.txt').filter(p=>p.y===fail.y),next_line:line(failed,fail.y+1),decline:find(failed,'The user declined')},
   fullscreen_hint:find(ask,'ctrl+f fullscreen'),minimize_hint:find(full,'ctrl+f minimize'),confirm_hint:find(ask,'enter confirm'),
   feedback_footer:find(feedback,'enter confirm'),feedback_start:find(feedback,'VIS36-FEEDBACK:'),feedback_end:find(feedback,'this correction.'),
   settings_category:find(settings,'Session'),settings_permission:find(settings,'Permissions'),settings_value:find(settings,'auto accept'),settings_search:find(settings,'Search'),
   url_title_rows:Array.from({length:urlLabel.y-urlTitle.y},(_,i)=>({y:urlTitle.y+i,line:line(url,urlTitle.y+i)})),url_value_rows:[urlLabel.y,urlLabel.y+1,urlLabel.y+2].map(y=>({y,line:line(url,y)}))};
 }
  const a=sides.upstream.failed_title,b=sides.oc.failed_title;
  const targeted_failed_card_pass=a.count===1&&b.count===1&&a.filename_on_title&&b.filename_on_title&&JSON.stringify(a.filename_positions)===JSON.stringify(b.filename_positions)&&a.line===b.line&&a.next_line===b.next_line&&JSON.stringify(a.decline)===JSON.stringify(b.decline);
  report.runs.push({name,sides,targeted_failed_card_pass});
}
report.targeted_failed_card_status=report.runs.every(r=>r.targeted_failed_card_pass)?'PASS':'FAIL';
fs.writeFileSync(output,JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify(report,null,2));
if(report.targeted_failed_card_status!=='PASS')process.exitCode=1;
