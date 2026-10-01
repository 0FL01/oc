# Native webfetch renderer provenance

`render.rs` uses the streaming tokenizer from **html5ever =0.35.0**,
https://github.com/servo/html5ever (MIT OR Apache-2.0), via crates.io's
checksum-pinned Cargo.lock. Existing dependencies were not upgraded. The parser
and all 19 newly resolved packages were already cached; offline build succeeded,
with no dependency download or product network call. HTML parsing/entity/raw-text
states belong to this vetted parser. Native code owns only bounded rendering.

Why this dependency: the prior extractor is a handwritten tag scanner; it cannot
provide the required safe, structured Markdown conversion. No HTML parser was in
the existing workspace dependency graph. A streaming tokenizer avoids an
unbounded DOM and permits 8KiB input / 4KiB text checkpoints. Body remains 1MiB,
output including metadata remains 1MiB; frame depth1024, language64 characters,
URLs8KiB, MIME256 bytes. Deep unsupported nesting fails explicitly. Conversion
runs on the existing Tokio blocking pool and joins before a terminal result;
deadline/cancel do not preempt an individual parser instruction or OS syscall.

The application exposes the existing loopback exception only with the separate
explicit test flag `OC_TEST_WEBFETCH_ALLOW_LOOPBACK=1`. Provider's test flag is
independent; neither flag admits other private addresses. No model/config field
can supply this exception. Default normal binary refusal is counted in TOOL17.

New transitive packages (Cargo.lock retains exact versions/checksums):
`futf 0.1.5`, `mac 0.1.1`, `markup5ever 0.35.0`, `match_token 0.35.0`,
`new_debug_unreachable 1.0.6`, `phf/phf_codegen/phf_generator/phf_shared 0.11.3`,
`precomputed-hash 0.1.1`, `rand 0.8.6`, `rand_core 0.6.4`, `siphasher 1.0.3`,
`string_cache 0.8.9`, `string_cache_codegen 0.5.4`, `tendril 0.4.3`,
`utf-8 0.7.6`, `web_atoms 0.1.3`. Cargo metadata confirms MIT for
new_debug_unreachable/phf family/precomputed-hash; remaining new packages declare
MIT or Apache-2.0 alternatives. Existing rand0.10.3/rand_core0.10.1 are unchanged.

Structure policy (headings, lists, code fences, links, tables, suppressed content)
is adapted from OpenCode v2.0.12, pinned
`2670273ff17da96f85c5826ced57aa1b368754fa`,
`packages/core/src/tool/html-markdown.ts` and `plugin/webfetch.ts`:
https://github.com/anomalyco/opencode/tree/2670273ff17da96f85c5826ced57aa1b368754fa
Native output is useful structured Markdown, not byte-identical donor rendering;
irregular/nested tables are a readable pipe-separated fallback, whitespace differs.
No donor browser/Cloudflare retry, image attachment, managed full-output store or
JS runtime was added. Non-HTML bodies stay original decoded text in all formats.
Public legacy fetch/extract APIs retain their previous text/JSON behavior.

## Donor notice

MIT License

Copyright (c) 2025 opencode

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
