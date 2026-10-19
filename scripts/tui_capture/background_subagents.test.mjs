import test from 'node:test';
import assert from 'node:assert/strict';
import {compareBackgroundCells} from './background_compare.mjs';
import {backgroundFacts,backgroundLaunch,finishBackgroundChildren} from './background_subagents.mjs';

test('private launch uses each real CLI contract with isolated data and temporary roots',()=>{
  for(const [origin,argument] of [['upstream','--standalone'],['oc','tui']]) {
    const home='/owned/fixture/'+origin+'/home';
    const launch={argv:['/owned/binary',argument],env:{HOME:home,TMPDIR:home+'/tmp',XDG_DATA_HOME:home+'/data'}};
    assert.equal(backgroundLaunch(launch,origin),home+'/tmp');
    assert.throws(()=>backgroundLaunch({...launch,argv:['/owned/binary','--server','shared']},origin));
    assert.throws(()=>backgroundLaunch({...launch,env:{...launch.env,TMPDIR:'/tmp'}},origin));
    assert.throws(()=>backgroundLaunch({...launch,env:{...launch.env,XDG_DATA_HOME:'/user/data'}},origin));
  }
});
test('terminal waits for both durable admissions, while consumed releases before the next step and A',async()=>{
  for(const backgroundCase of ['terminal','wrap','consumed']) {
    const order=[];
    const finished=await finishBackgroundChildren({backgroundCase,
      logs:[{kind:'background_parent_flushed'}],
      command:async action=>order.push(action),
      observe:async(label,predicate)=>{
        order.push(label);
        const facts={notices:['B','A'].map(key=>({key,location:label==='both-admitted'?'inbox':'message'}))};
        assert(predicate(facts));
        return {facts};
      },
      shot:async label=>order.push(label),
    });
    assert.equal(finished.facts.notices.length,2);
    assert.deepEqual(order,backgroundCase!=='consumed'
      ?['release-A','both-admitted','release-parent','both-completed']
      :['release-parent','background-parent-next','release-A','both-completed']);
  }
});
const descriptions={A:'Сравнение subagent с upstream',B:'Состояние Git и тесты'};
const cell=symbol=>({symbol,width:1,fg:'#eeeeee',bg:'#131313',modifiers:[]});
function frame() {
  const lines=['','   ✓ Explore Subagent — '+descriptions.A+'  Background ',
    '   ✓ Explore Subagent — '+descriptions.B+'  Background ','','   PARENT-STEP','','   ↳ Explore finished · '+descriptions.B,'','   ↳ Explore finished · '+descriptions.A];
  return {rows:24,cols:120,cells:Array.from({length:24},(_,y)=>[...(lines[y]||'').padEnd(120)].map(cell))};
}
test('complete original cards, badge styles, missing peer and notice are qualified, not just first glyph',()=>{
  const reference=frame(), actual=structuredClone(reference);
  assert(Object.values(compareBackgroundCells(reference,actual,'completed')).every(region=>region.styled_cells_equal&&region.relative_anchor_equal));
  const x=actual.cells[1].findIndex((_,i)=>actual.cells[1].slice(i,i+10).map(cell=>cell.symbol).join('')==='Background');
  actual.cells[1][x].bg='#ffffff';
  assert.equal(compareBackgroundCells(reference,actual,'completed')['card-A'].styled_cells_equal,false);
  const missingPeer=frame();missingPeer.cells[2]=Array.from({length:120},()=>cell(' '));
  assert.throws(()=>compareBackgroundCells(reference,missingPeer,'completed'),/unique/);
  const missingNotice=frame();missingNotice.cells[6]=Array.from({length:120},()=>cell(' '));
  assert.throws(()=>compareBackgroundCells(reference,missingNotice,'completed'),/notice/);
});
test('current TS provider call, relational child and distinct synthetic notice identities are mandatory',()=>{
  const issued=Object.fromEntries(['A','B'].map(key=>[key,{call_id:'call_'+key,arguments:{agent:'explore',description:descriptions[key],background:true,prompt:JSON.stringify({child:key})}}]));
  const data={schema:'ts',root:'parent',children:['A','B'].map(key=>({id:'child_'+key,parent_id:'parent'})),inbox:[],messages:[{type:'assistant',data:{content:['A','B'].map(key=>({type:'tool',id:'call_'+key,name:'subagent',state:{status:'completed',metadata:{sessionID:'child_'+key,status:'running'},input:Object.fromEntries(Object.entries(issued[key].arguments).reverse())}}))}}]};
  const facts=()=>backgroundFacts({composer:{issued,background:data}});
  assert.equal(facts().children.B.child,'child_B');
  data.messages.push({id:'notice_B',type:'synthetic',seq:4,data:{description:descriptions.B,metadata:{source:'subagent',childID:'child_B',state:'completed'}}});
  assert.equal(facts().notices[0].id,'notice_B');
  data.children[1].parent_id='foreign';
  assert.throws(facts,/relational/);
});
