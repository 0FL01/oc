// Official npm @opencode/cli-linux-x64@2.0.31; no rebuilt/modified oracle.
// Immutable release files were fetched and compared with the supplied checkout.
// All listed files match byte-for-byte except index.tsx: its three differences
// are confined to unrelated session /copy commands, not tool/group/dialog code.
export const executeReference = {
  classification:'SUPPLEMENTARY_EXECUTE_EXPLORATION_ONLY', version:'2.0.31',
  release_commit:'22728d67461260d56bb9796b9b8f36e1de47202e',
  donor_commit:'bc5ebf00a4c169ece10f276e6ce02eaf18c326e6',
  binary_sha256:'2df936a20cf74af5706020342164628f9a31978c50c2b0ad306b42b614fe245d',
  release_source_base:'https://raw.githubusercontent.com/anomalyco/opencode/22728d67461260d56bb9796b9b8f36e1de47202e/',
  release_session_sha256:'cce1787496512743d89733ae982b1f38e08585eb1bf7c762ae815a4dfc2fec27',
  session_delta:'Only messages.copy/session.copy command registration and compaction-summary copy; scoped render/producer functions unchanged',
  source_hashes:{
    'packages/tui/src/routes/session/index.tsx':'d7a7f5ba7a7714c4bdd596fd726c2091d49109d99095454379474a7f11150985',
    'packages/tui/src/routes/session/dialog-execute.tsx':'3b653e2ea00bce2af77be7dc97bc58269a78ed23353eb7db6410245fbe337f0a',
    'packages/tui/src/routes/session/group-view.tsx':'be9eb25132ad817ca7d3081b2b199ded0f819c386affd700bae1196b710ae69a',
    'packages/tui/src/routes/session/grouping/session.ts':'701765c52dbccd3649bfd00dfc95abe38d11f94f5f8bf54d6c5a6b7f41fe7b18',
    'packages/tui/src/routes/session/message-parts.tsx':'b9db21e55dc5a0859e6711c229b1108b95cbf4e3b5d2bf2722cafcb5effa6bc5',
    'packages/tui/src/util/tool-display.ts':'07d3a0cf620b437a0da227e5870f379f531d40042ca72e607487e6ed8028b9c3',
    'packages/tui/src/util/collapse-tool-output.ts':'593aa5c4325991af7382d4635ea8bdcd2021a00727298a251fe556620a9f0e19',
    'packages/tui/src/parsers-config.ts':'ce67b2190e1b2c755853484e914398184703f82fd6c5d4932bb1a7f27734123e',
    'packages/tui/src/ui/dialog.tsx':'f11539112398ba737d693c90d06ed571df41edba186b743fd5128c514ebb5ed4',
    'packages/tui/src/context/clipboard.tsx':'48e3e43be1665c539c31795b1f86493512fddcd701a687f4284bbbf0f9c02ceb',
    'packages/tui/src/context/theme.tsx':'5ed5f57ed2b62b9d7dd4368d574ed7176c4b13acd36e8c1645c098f80de2b155',
    'packages/core/src/codemode/tool.ts':'64643a641f91bd9c48424d4f41e586703719cc81824abefb42e27eb8e542f934',
    'packages/core/src/codemode/web.ts':'e80e6d535b48cedbb31e4c3463102d3911016e726ff538530e92ea0fe8e2a06e',
    'packages/codemode/src/tool-runtime.ts':'5588ade78704bfaf2af9a0d1bf529b5443cd3bd1550e5b9ee6aab5d36870fc46',
    'packages/codemode/src/interpreter/globals.ts':'7fb1df8bc2e2fc8b91387833691db7718e1d0cd8fd822115bf82eb064f591fd7',
  },
  syntax:{opentui:'0.5.17', scope:'Bundled TypeScript plus pinned URL-cache JSON only',
    download_utils_sha256:'2dac76d9c2b98934d10cd7cc857f8a07e22d65e9fdfed51721c24a0c9a996c1c',
    json_wasm_sha256:'d2119fb98d5912719b13f9458574f8608d2d29dfbe45f6be1f860ea1fe2a2405',
    json_highlights_sha256:'6ab09656820f0e8dac860c96dced341221284544c05308ee337727f5db937101'},
};
