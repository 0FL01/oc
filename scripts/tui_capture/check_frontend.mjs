#!/usr/bin/env node
// Synthetic adapter qualification, never labelled as an application capture.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const here=path.dirname(fileURLToPath(import.meta.url));
const tools=process.argv[2] || '/home/opencode/.cache/opencode-tmp/opencode/t44-reference';
process.env.PLAYWRIGHT_BROWSERS_PATH=path.join(tools,'browsers');
const require=createRequire(path.join(tools,'package.json'));
const {chromium}=require('playwright');
const home=path.join(tools,'frontend-check-home');
fs.mkdirSync(home,{recursive:true});
const browser=await chromium.launch({headless:true,env:{HOME:home,PATH:'/usr/bin:/bin',LANG:'C.UTF-8'}});
try {
  const page=await browser.newPage();
  await page.setContent('<div id="terminal"></div>');
  await page.addStyleTag({path:path.join(tools,'node_modules/@xterm/xterm/css/xterm.css')});
  await page.addScriptTag({path:path.join(tools,'node_modules/@xterm/xterm/lib/xterm.js')});
  await page.addScriptTag({path:path.join(tools,'node_modules/@xterm/addon-unicode11/lib/addon-unicode11.js')});
  await page.addScriptTag({path:path.join(here,'frontend.js')});
  const replies=[];
  await page.exposeFunction('terminalReply',s=>replies.push(s));
  await page.evaluate(()=>startTerminal({columns:12,rows:4}));
  const write=async s=>page.evaluate(d=>writeTerminal(d),Buffer.from(s).toString('base64'));
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
  console.log('PASS: real xterm buffer preserves RGB, styled blanks, 8 attributes, CJK continuation, combining text, cursor and DSR reply');
} finally {await browser.close();}
