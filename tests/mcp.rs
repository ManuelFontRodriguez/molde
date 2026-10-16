//! The context module speaks MCP: drive the JSON-RPC handler through the same
//! message sequence a real client sends, against the demo site.
use molde::mcp::McpServer;
use serde_json::{json, Value};
use std::path::Path;

fn server() -> McpServer {
    McpServer {
        site: Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/demo"),
        out: std::env::temp_dir().join(format!("molde-mcp-{}", std::process::id())),
    }
}

/// Extract the JSON a tool call returned inside result.content[0].text.
fn tool_json(resp: &Value) -> Value {
    assert_eq!(resp["result"]["isError"], false, "tool errored: {resp}");
    serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn handshake_and_tool_list() {
    let s = server();

    let resp = s
        .handle(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": "2025-06-18", "capabilities": {},
                        "clientInfo": { "name": "test", "version": "0" } },
        }))
        .unwrap();
    assert_eq!(resp["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(resp["result"]["serverInfo"]["name"], "molde");

    // notifications get no response
    assert!(s
        .handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .is_none());

    let resp = s
        .handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }))
        .unwrap();
    let names: Vec<&str> = resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "check",
            "list_routes",
            "explain_route",
            "render_page",
            "diff_build"
        ]
    );
}

#[test]
fn check_tool_returns_full_report() {
    let s = server();
    let resp = s
        .handle(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "check", "arguments": {} },
        }))
        .unwrap();
    let report = tool_json(&resp);
    assert_eq!(report["verdict"], "pass");
    assert_eq!(report["build"]["pages"], 9);
    assert_eq!(report["errors"].as_array().unwrap().len(), 0);

    // a second identical check is DEDUPLICATED into the same build, so there is
    // still only one build and diff_build reports that as feedback, not a crash
    let resp = s
        .handle(&json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "check", "arguments": {} },
        }))
        .unwrap();
    assert_eq!(tool_json(&resp)["verdict"], "pass");
    let resp = s
        .handle(&json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "diff_build", "arguments": {} },
        }))
        .unwrap();
    assert_eq!(resp["result"]["isError"], true);
    let err: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(
        err["error"].as_str().unwrap().contains("nothing to diff"),
        "unexpected error: {err}"
    );

    // an id that is a path is rejected as feedback, never read from disk
    let resp = s
        .handle(&json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": { "name": "diff_build", "arguments": { "from": "../..", "to": "/" } },
        }))
        .unwrap();
    assert_eq!(resp["result"]["isError"], true);
    let err: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(
        err["error"].as_str().unwrap().contains("not found"),
        "unexpected error: {err}"
    );

    let _ = std::fs::remove_dir_all(&s.out);
}

#[test]
fn list_explain_render() {
    let s = server();

    let resp = s
        .handle(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "list_routes", "arguments": {} },
        }))
        .unwrap();
    let routes = tool_json(&resp);
    assert_eq!(routes["total_pages"], 9);
    assert_eq!(routes["routes"].as_array().unwrap().len(), 4);
    assert_eq!(routes["routes"][1]["foreach"], "books");
    assert_eq!(routes["routes"][1]["pages"], 4);
    assert_eq!(routes["routes"][1]["lastmod"], "{{ item.updated }}");
    assert_eq!(routes["routes"][2]["group_by"], "genre");
    assert_eq!(routes["routes"][2]["pages"], 3);
    assert_eq!(routes["routes"][3]["index"], false);
    assert_eq!(routes["routes"][3]["noindex_pages"], 1);

    let resp = s
        .handle(&json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "explain_route", "arguments": { "path": "/books/dune/" } },
        }))
        .unwrap();
    let explained = tool_json(&resp);
    assert_eq!(explained["matched"], true);
    assert_eq!(explained["template"], "book.html.j2");
    assert_eq!(explained["item"]["id"], "dune");
    assert_eq!(explained["index"], true);
    assert_eq!(explained["lastmod"], "2026-09-18");

    let resp = s
        .handle(&json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "render_page", "arguments": { "path": "/books/dune/" } },
        }))
        .unwrap();
    let rendered = tool_json(&resp);
    let html = rendered["html"].as_str().unwrap();
    assert!(html.contains("Frank Herbert"));
    assert!(
        html.contains("Reserved — join the waiting list"),
        "reserved badge missing"
    );

    // unmatched path is feedback, not a crash
    let resp = s
        .handle(&json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": { "name": "explain_route", "arguments": { "path": "/nope/" } },
        }))
        .unwrap();
    assert_eq!(tool_json(&resp)["matched"], false);

    // unknown tool is a protocol error
    let resp = s
        .handle(&json!({
            "jsonrpc": "2.0", "id": 5, "method": "tools/call",
            "params": { "name": "bogus", "arguments": {} },
        }))
        .unwrap();
    assert_eq!(resp["error"]["code"], -32602);
}

#[test]
fn bad_lines_get_a_parse_error_and_the_server_keeps_going() {
    let s = server();
    let mut input = Vec::new();
    input.extend_from_slice(b"not json\n");
    input.extend_from_slice(&[0xff, 0xfe, b'\n']); // not UTF-8: used to end the server
    input.extend_from_slice(b"\n");
    input.extend_from_slice(br#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#);
    input.push(b'\n');
    let mut output = Vec::new();
    s.serve(&input[..], &mut output).unwrap();
    let lines: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert_eq!(lines[0]["error"]["code"], -32700);
    assert_eq!(lines[1]["error"]["code"], -32700);
    assert_eq!(lines[2]["id"], 7);
    assert_eq!(lines[2]["result"], json!({}));
}
