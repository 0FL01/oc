// Development-only probe of the actual pinned Web Tree-sitter grammar assets.
// No query substitution, inheritance expansion, native token golden or runtime
// asset downloader is introduced into oc.
import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import {Parser,Language,Query} from '/home/opencode/.cache/opencode-tmp/opencode/syntax-reference-sdk/node_modules/web-tree-sitter/tree-sitter.js';

const root=path.resolve(import.meta.dirname,'..');
const cache='/home/opencode/.cache/opencode-tmp/opencode/syntax-reference-wasm';
const bundled='/home/opencode/.cache/opencode-tmp/opencode/t44-opentui-0.5.10/packages/core/src/lib/tree-sitter/assets';
const manifest=JSON.parse(await fs.readFile(path.join(root,'crates/oc-tui/assets/syntax/manifest.json'),'utf8'));
const selected=new Set((process.argv[2]??'').split(',').filter(Boolean));
await fs.mkdir(cache,{recursive:true});
await Parser.init();
for(const entry of manifest.grammars.filter(e=>!selected.size||selected.has(e.name))) {
  const filename=path.join(cache,entry.name+'.wasm');
  let bytes;
  try {bytes=await fs.readFile(filename);}
  catch(error) {
    if(error.code!=='ENOENT')throw error;
    const shipped=path.join(bundled,entry.name,'tree-sitter-'+entry.name+'.wasm');
    try {bytes=await fs.readFile(shipped);}
    catch(error) {
      if(error.code!=='ENOENT')throw error;
      const response=await fetch(entry.reference_wasm,{signal:AbortSignal.timeout(30000)});
      if(!response.ok)throw Error(entry.name+': reference download '+response.status);
      bytes=Buffer.from(await response.arrayBuffer());
      if(bytes.length>16*1024*1024)throw Error('Oversized reference grammar');
    }
    await fs.writeFile(filename,bytes,{flag:'wx'});
  }
  const language=await Language.load(bytes);
  const parser=new Parser();parser.setLanguage(language);
  const source=process.argv[3]??'let value = 42;\n';
  const tree=parser.parse(source);
  const querySource=await fs.readFile(path.join(root,'crates/oc-tui/assets/syntax',entry.queries.highlights.file),'utf8');
  let queryStatus;
  try {const query=new Query(language,querySource);const captures=query.captures(tree.rootNode);queryStatus={status:'loaded',captures:captures.length,
    tokens:captures.map(c=>({scope:c.name,start:Buffer.byteLength(source.slice(0,c.node.startIndex),'utf8'),end:Buffer.byteLength(source.slice(0,c.node.endIndex),'utf8')}))};query.delete();}
  catch(error) {queryStatus={status:'unavailable',error:error.message};}
  console.log(JSON.stringify({name:entry.name,reference_wasm:entry.reference_wasm,
    wasm_sha256:crypto.createHash('sha256').update(bytes).digest('hex'),
    version:language.abiVersion,query:queryStatus,
    node_type_count:language.nodeTypeCount,
    unsafe_expression:language.types.includes('unsafe_expression'),
    sample_tree:tree.rootNode.toString()}));
  tree.delete();parser.delete();
}
