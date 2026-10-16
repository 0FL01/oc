# Bundled review prompt

`v2/review.txt` is a byte-for-byte copy of
`packages/core/src/plugin/command/review.txt` from
<https://github.com/anomalyco/opencode/tree/v2.0.12>, pinned tree
`2670273ff17da96f85c5826ced57aa1b368754fa`.
SHA-256: `727292c08ee718fff8852ec91408a255ce08c716b3d46cc56be3d399589e3479`.
`crates/oc-adapters/src/composition.rs` loads it only when no admitted workspace
`review` definition exists. The pinned template has no `${path}` token, so the
original command's Location replacement has no effect and none is added here.

Copyright (c) 2025 opencode. Licensed under the upstream MIT License (`LICENSE`
at the pinned tag):

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
