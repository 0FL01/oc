#!/usr/bin/env node
// Synthetic adapter qualification, never labelled as an application capture.
// Actual pinned browser/parser/renderers, not a replacement terminal model.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const here=path.dirname(fileURLToPath(import.meta.url));
const tools=path.resolve(process.argv[2]||'/home/opencode/.cache/opencode-tmp/opencode/t44-reference');
const webgl=path.resolve(process.argv[3]||'/home/opencode/.cache/opencode-tmp/opencode/cursor-renderer');
process.env.PLAYWRIGHT_BROWSERS_PATH=path.join(tools,'browsers');
const require=createRequire(path.join(tools,'package.json'));
const {chromium}=require('playwright');
const home=path.join(tools,'frontend-check-home');
fs.mkdirSync(home,{recursive:true});
const browser=await chromium.launch({headless:true,env:{HOME:home,PATH:'/usr/bin:/bin',LANG:'C.UTF-8'}});
try {
  for(const renderer of ['dom','webgl']) {
    const page=await browser.newPage({viewport:{width:400,height:200}});
    try {
      await page.setContent('<div id="terminal"></div>');
      await page.addStyleTag({path:path.join(tools,'node_modules/@xterm/xterm/css/xterm.css')});
      for(const script of ['@xterm/xterm/lib/xterm.js','@xterm/addon-unicode11/lib/addon-unicode11.js'])
        await page.addScriptTag({path:path.join(tools,'node_modules',script)});
      await page.addScriptTag({path:path.join(webgl,'node_modules/@xterm/addon-webgl/lib/addon-webgl.js')});
      const replies=[];
      await page.exposeFunction('terminalReply',s=>replies.push(s));
      await page.addScriptTag({path:path.join(here,'frontend.js')});
      await page.evaluate(renderer=>startTerminal({columns:12,rows:4,cursor_renderer:renderer}),renderer);
      const write=async text=>{
        await page.evaluate(data=>writeTerminal(data),Buffer.from(text).toString('base64'));
        await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
      };
      await write('\x1b[38;2;18;52;86;48;2;101;67;33;1;2;3;4;5;7;8;9mA 界\x1b[0mе́');
      await write('\x1b[3;5H\x1b[6 q\x1b[?25l\x1b[6n');
      let f=await page.evaluate(()=>readTerminal());
      assert.equal(f.cells.length,4);assert.equal(f.cells[0].length,12);
      assert.deepEqual(f.cells[0][0],{symbol:'A',fg:'#123456',bg:'#654321',width:1,
        modifiers:['bold','crossed_out','dim','hidden','italic','reversed','slow_blink','underlined']});
      assert.equal(f.cells[0][1].symbol,' ');assert.equal(f.cells[0][1].bg,'#654321');
      assert.equal(f.cells[0][2].symbol,'界');assert.equal(f.cells[0][2].width,2);
      assert.equal(f.cells[0][3].width,0);assert.equal(f.cells[0][3].symbol,'');
      assert.equal(f.cells[0][4].symbol,'е́');
      assert.deepEqual(f.cursor,{x:4,y:2,visible:false,shape:'bar'});
      assert.ok(replies.includes('\x1b[3;5R'));
      await write('\x1b[?25h\x1b[4 q');
      f=await page.evaluate(()=>readTerminal());
      assert.deepEqual(f.cursor,{x:4,y:2,visible:true,shape:'underline'});
      console.log(`PASS ${renderer}: RGB, styled blanks, 8 attributes, CJK continuation, combining text, cursor and DSR reply`);
      await write('\x1bc');
      await page.evaluate(()=>term.resize(8,3));
      await write('ABCDEFGH');
      const measured=await page.evaluate(()=>{
        const grid=readTerminal(),geometry=readCaptureGeometry();
        let painted_x;
        if(window.cursorWebgl) painted_x=cursorWebgl._renderer._model.cursor.x;
        else {
          const cursor=document.querySelector('.xterm-cursor').getBoundingClientRect();
          painted_x=Math.round((cursor.x-geometry.screen_rect.x)/geometry.measured_cell_width);
        }
        return {grid,logical:geometry.logical_buffer_cursor,painted_x};
      });
      assert.deepEqual(measured.logical,{x:8,y:0});
      assert.equal(measured.grid.cursor.x,7);
      assert.equal(measured.painted_x,7);
      assert.equal(measured.grid.cursor.visible,true);
      assert.equal(measured.grid.cells[0].map(c=>c.symbol).join(''),'ABCDEFGH');
      await write('\x1b[?25l');
      const hidden=await page.evaluate(()=>readTerminal());
      assert.equal(hidden.cursor.visible,false);
      assert.equal(hidden.cursor.x,7);
      await write('\x1b[?25h\r\n');
      const moved=await page.evaluate(()=>readTerminal());
      assert.equal(moved.cursor.x,0);assert.equal(moved.cursor.y,1);
      const refused=await page.evaluate(()=>{
        term._core._bufferService.buffer.x=9;
        try {readTerminal();return false;} catch(e) {return e.message==='Invalid logical buffer cursor';}
      });
      assert.equal(refused,true);
      console.log(JSON.stringify({renderer,pending_wrap_raw_x:8,actual_painted_x:7,
        physical_cursor_matches:true,all_cells_retained:true,invalid_state_rejected:true}));
    } finally {await page.close();}
  }
} finally {await browser.close();}
