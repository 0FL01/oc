# T37 initial regressions

Base HEAD: `839279c` (T36 closeout). All roots temporary; transports were loopback
or local fixture processes. No live/paid calls or real credentials.

Before production edits:

```sh
cargo test -p oc --test mcp_application -- --nocapture
```

Exit 101, **0 passed / 5 failed**:

- exact user `headers.Authorization` failed MCP attach;
- conflicting `Authorization`/`authorization` had no actionable conflict error;
- two TUI turns spawned/initialized/listed stdio MCP twice;
- collision-renamed `a__b__c__2` routed as unknown;
- image-only result became empty successful output.

The first-red bounded log was stored outside the repository at
`/home/opencode/.cache/opencode-tmp/opencode/T37-mcp-application-first-red.log`.
Later assertions were retained and expanded for catalog limits and strict
`query`/`response_length` search arguments.
