// Test-only original oracle. Install fuzzysort@3.1.0 in the isolated tools root.
// stdout is the fixture; no dependency on Node or upstream in Cargo tests/runtime.
import {createRequire} from 'node:module';
const require = createRequire('/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package.json');
const fuzzy = require('fuzzysort');
if (require('fuzzysort/package.json').version !== '3.1.0') throw Error('wrong oracle');
const options = [
  ['Switch model', 'Agent', 'model.list'], ['Switch model variant', 'Agent', 'variant.list'],
  ['Switch session', 'Session', 'session.list'], ['New session', 'Session', 'session.new'],
  ['Default', '', ''], ['none', '', ''], ['fast', '', ''], ['fast', '', ''],
  ['CheatManager.h', '', ''], ['Manifest.cpp', '', ''], ['fooBar', '', ''],
  ['foobar', '', ''], ['Foo Bar', '', ''], ['strawberry', '', ''],
  ['Straw Berry', '', ''], ['Код модель', '', ''], ['Café déjà vu', '', ''],
  ['かなガイド', '', ''], ['🚀 rocket model', '', ''], ['Modal 29', '', ''],
  ['Other', 'Real provider', 'supplemental'], ['abc abc', 'abc', ''],
  ['abc', '', ''], ['xabc', '', ''], ['a b c', '', ''], ['abcd', '', ''],
];
const queries = ['swmdl', 'model', 'agent', 'session new', 'c man', 'fb', 'foo bar',
  'straw berry', 'berry straw', 'cafe deja', 'км', 'かな', '🚀m', 'md29', 'nope', 'abc',
  'abc abc', '  abc ', 'real other', 'supple', 'fast', ' a\tb ', 'фоо', 'switch agent'];
const cases = [];
for (const query of queries) for (const weighted of [false, true]) {
  const threshold = weighted ? 0.7 : 0;
  const objs = options.map((k, index) => ({title:k[0], category:k[1], searchText:k[2], index}));
  const results = fuzzy.go(query, objs, {keys:weighted ? ['title','category','searchText'] : ['title','category'],
    threshold, ...(weighted ? {scoreFn:r=>r[0].score*2+r[1].score+r[2].score} : {})});
  cases.push({query, weighted, threshold,
    results:results.map(r=>({index:r.obj.index,score:r.score}))});
}
console.log(JSON.stringify({options, cases}));
