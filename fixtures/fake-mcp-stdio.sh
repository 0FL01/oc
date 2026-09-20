#!/bin/sh
# Pinned fake MCP stdio server for T21 (MCP04/MCP05). POSIX sh only, no
# Node, no browser, no network. Speaks line-delimited JSON-RPC on stdio.
set -u

echo "fake-mcp-stdio ready" >&2
echo "SECRET_TOKEN=fake-secret-123" >&2
i=0
while [ "$i" -lt 1024 ]; do
    echo "pad-0123456789abcdef0123456789abcdef0123456789abcdef01234567" >&2
    i=$((i + 1))
done

json_escape() {
    sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' | awk '{printf "%s\\n", $0}'
}

while IFS= read -r line; do
    method=$(printf '%s' "$line" | sed -n 's/.*"method"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
    id=$(printf '%s' "$line" | sed -n 's/.*"id"[[:space:]]*:[[:space:]]*\(\("[^"]*"\|-*[0-9][0-9]*\)\).*/\1/p')
    [ -z "$id" ] && id="null"
    case "$method" in
        initialize)
            result='{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"fake-stdio","version":"test"}}'
            ;;
        notifications/initialized)
            continue
            ;;
        tools/list)
            result='{"tools":[{"name":"env","description":"dump environment","inputSchema":{"type":"object"}},{"name":"args","description":"echo argv","inputSchema":{"type":"object"}},{"name":"boom","description":"always fails","inputSchema":{"type":"object"}}]}'
            ;;
        tools/call)
            name=$(printf '%s' "$line" | sed -n 's/.*"name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
            case "$name" in
                env)
                    text=$(env | sort | json_escape)
                    result=$(printf '{"content":[{"type":"text","text":"%s"}],"isError":false}' "$text")
                    ;;
                args)
                    text=$(printf '%s\n' "$@" | json_escape)
                    result=$(printf '{"content":[{"type":"text","text":"%s"}],"isError":false}' "$text")
                    ;;
                boom)
                    result='{"content":[{"type":"text","text":"nope"}],"isError":true}'
                    ;;
                *)
                    text="stdio:$name"
                    result=$(printf '{"content":[{"type":"text","text":"%s"}],"isError":false}' "$text")
                    ;;
            esac
            ;;
        *)
            printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"unknown"}}\n' "$id"
            continue
            ;;
    esac
    printf '{"jsonrpc":"2.0","id":%s,"result":%s}\n' "$id" "$result"
done
