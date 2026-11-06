"""Offline protocol fixture. Only binds loopback and never calls external services."""
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

transport, mode = sys.argv[1:3]
legacy = mode.startswith("legacy")


def response(request, initialized=False):
    method = request.get("method")
    identifier = request.get("id")
    if identifier is None:
        return None

    def error(code):
        return {"jsonrpc": "2.0", "id": identifier, "error": {"code": code, "message": "mock error"}}

    if method == "server/discover":
        if legacy:
            if transport == "stdio":
                sys.exit(0)  # Old implementations can exit on a pre-init request.
            return error(-32601)
        if mode == "unsupported":
            value = error(-32022)
            value["error"]["data"] = {"supported": ["2099-01-01"]}
            return value
        return {"jsonrpc": "2.0", "id": identifier, "result": {
            "resultType": "complete", "supportedVersions": ["2026-07-28"],
            "capabilities": {} if mode == "no-tools" else {"tools": {}, "resources": {}}
        }}
    if method == "initialize":
        if not legacy:
            return error(-32601)
        return {"jsonrpc": "2.0", "id": identifier, "result": {
            "protocolVersion": "2099-01-01" if mode == "legacy-bad-version" else "2025-03-26",
            "capabilities": {"tools": {}, "resources": {}}, "serverInfo": {"name": "mock", "version": "1"}
        }}
    if legacy and not initialized:
        return error(-32600)
    if not legacy:
        meta = request.get("params", {}).get("_meta", {})
        assert meta.get("io.modelcontextprotocol/protocolVersion") == "2026-07-28"
        assert meta.get("io.modelcontextprotocol/clientCapabilities") == {}
        assert meta.get("io.modelcontextprotocol/clientInfo", {}).get("name") == "mcp-hub"
    if method == "tools/list":
        if mode == "tool-error":
            return error(-32603)
        if mode == "delay":
            time.sleep(3)
        if mode == "no-tools":
            return error(-32601)
        result = {"resultType": "complete", "tools": [{"name": "first"}]}
        if mode in ("pages", "cursor-loop"):
            if request.get("params", {}).get("cursor"):
                result["tools"] = [{"name": "second"}]
            if not request.get("params", {}).get("cursor") or mode == "cursor-loop":
                result["nextCursor"] = "page-two"
        return {"jsonrpc": "2.0", "id": identifier, "result": result}
    if method == "resources/list":
        return {"jsonrpc": "2.0", "id": identifier, "result": {"resultType": "complete", "resources": []}}
    return error(-32601)


if transport == "stdio":
    initialized = False
    for line in sys.stdin:
        request = json.loads(line)
        if request.get("method") == "notifications/initialized":
            initialized = True
        value = response(request, initialized)
        if value is not None:
            if mode == "notifications":
                print(json.dumps({"jsonrpc": "2.0", "method": "notifications/progress", "params": {}}), flush=True)
                print(json.dumps({"jsonrpc": "2.0", "id": 9999, "result": {}}), flush=True)
            print(json.dumps(value), flush=True)
else:
    class Handler(BaseHTTPRequestHandler):
        initialized = False

        def log_message(self, *args):
            pass

        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            method = request["method"]
            assert self.headers.get("Mcp-Method") == method
            assert "application/json" in self.headers["Accept"]
            assert "text/event-stream" in self.headers["Accept"]
            assert self.headers.get("Mcp-Name") is None
            if mode == "auth" or mode == "forbidden" or mode == "server-error":
                self.send_response({"auth": 401, "forbidden": 403, "server-error": 503}[mode])
                self.end_headers()
                return
            if mode == "headers":
                assert self.headers.get("Authorization") == "Bearer audit-only"
            if not legacy:
                assert self.headers.get("MCP-Protocol-Version") == "2026-07-28"
                assert self.headers.get("Mcp-Session-Id") is None
            if legacy and method not in ("server/discover", "initialize"):
                assert self.headers.get("Mcp-Session-Id") == "audit-session"
                assert self.headers.get("MCP-Protocol-Version") == "2025-03-26"
            if method == "notifications/initialized":
                Handler.initialized = True
                self.send_response(202)
                self.end_headers()
                return
            value = response(request, Handler.initialized)
            self.send_response(400 if legacy and method == "server/discover" else 200)
            self.send_header("Content-Type", "text/event-stream" if mode == "sse" else "application/json")
            if legacy and method == "initialize":
                self.send_header("Mcp-Session-Id", "audit-session")
            self.end_headers()
            if mode == "sse":
                self.wfile.write(b': keepalive\n\ndata: {"jsonrpc":"2.0","method":"notifications/progress"}\n\n')
                self.wfile.write(b'data: {"jsonrpc":"2.0","id":9999,"result":{}}\n\n')
                self.wfile.write(("data: " + json.dumps(value) + "\n\n").encode())
            else:
                self.wfile.write(json.dumps(value).encode())

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    print(server.server_port, flush=True)
    server.serve_forever()
