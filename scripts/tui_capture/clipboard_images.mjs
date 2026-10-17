import fs from 'node:fs';
import path from 'node:path';
import {mcpSidebarReference} from './mcp_sidebar.mjs';

export const clipboardReference = {...mcpSidebarReference,
  release_source_verification:'Clipboard acquisition, prompt insertion and attachment helpers byte-identical; session route differs only in unrelated messages.copy/session.copy commands (verified against official release commit).',
  source_hashes: {
  'packages/tui/src/clipboard.ts': '9f6e4bd731f8f742757aec8302c6b57c8ca5de194504a0e166d86c54fbbcde2f',
  'packages/tui/src/component/prompt/index.tsx': '9c2f84dadf3d15f41639d3dd7f73f102aecff35b1dd3b5c3297a4780844679fa',
  'packages/tui/src/routes/session/index.tsx': 'd7a7f5ba7a7714c4bdd596fd726c2091d49109d99095454379474a7f11150985',
  'packages/tui/src/prompt/attachment.ts': 'b9a07d882ab4efb8e52146d74dfe6ad35287509b7cc1fb4b2ab35d0e96a6374f',
}};

export async function probeClipboardImages({origin, dir, send, waitFor, capture, logs, prompt}) {
  const result = {origin, status:'RUNNING', classification:'SUPPLEMENTARY', stages:[]};
  const save = () => fs.writeFileSync(path.join(dir,'clipboard-images-checks.json'), JSON.stringify(result,null,2)+'\n');
  try {
    send(prompt + ' ', 'CLIPBOARD_TEXT_PREFIX');
    send('\x16', 'CLIPBOARD_IMAGE_PASTE');
    const draft = await waitFor(f => f.text.includes('[Image 1]'), 'actual clipboard image marker');
    if (logs.some(e => e.kind === 'provider')) throw Error('Draft paste generated a request');
    result.stages.push({stage:'draft', capture:await capture(`clipboard-image-draft-${draft.columns}`,draft,'CAPTURED_SUPPLEMENTARY')});
    send('\r', 'CLIPBOARD_IMAGE_SUBMIT');
    const sent = await waitFor(f => f.text.includes('clipboard') && f.text.includes('GEOMETRY-SHORT') &&
      logs.some(e => e.kind === 'provider_completed' && e.operation === 'transcript'), 'settled image message and chip');
    const requests = logs.filter(e => e.kind === 'provider' && e.operation === 'transcript');
    if (requests.length !== 1 || !requests[0].valid || !requests[0].clipboard_png_verified) throw Error('PNG did not reach actual provider');
    result.stages.push({stage:'sent', capture:await capture(`clipboard-image-sent-${sent.columns}`,sent,'CAPTURED_SUPPLEMENTARY')});
    if (result.stages.some(s => s.capture === 'UNSTABLE_CAPTURE')) throw Error('Unstable clipboard frame');
    send('/quit', 'CLIPBOARD_QUIT_DRAFT');
    await waitFor(f => f.text.includes('/quit'), 'painted quit command');
    send('\r', 'CLIPBOARD_QUIT');
    const deadline = Date.now() + 12000;
    while (!logs.some(e => e.kind === 'exit') && Date.now() < deadline)
      await new Promise(resolve => setTimeout(resolve, 25));
    if (!logs.some(e => e.kind === 'exit' && e.code === 0 && e.termination === 'natural')) throw Error('No clean natural exit');
    result.status = 'PASS'; save(); return result;
  } catch (error) {
    result.status = 'FAILED'; result.error = error.message; save(); throw error;
  }
}
