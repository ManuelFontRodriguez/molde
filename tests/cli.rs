//! The binary itself: what `molde --version` tells a bug report.
use std::process::Command;

#[test]
fn version_names_the_package_version_and_commit() {
    let out = Command::new(env!("CARGO_BIN_EXE_molde"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    let line = String::from_utf8(out.stdout).unwrap();
    let expected = format!("molde {}", env!("CARGO_PKG_VERSION"));
    assert!(line.starts_with(&expected), "{line}");
    // from a git checkout of molde, "(<sha> <date>)" follows; from a source tarball
    // (no .git) the version stands alone
    if std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".git")
        .exists()
    {
        assert!(
            line.trim_end().ends_with(')') && line.contains(" ("),
            "{line}"
        );
    } else {
        assert_eq!(line.trim_end(), expected);
    }
}

fn demo_site() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/demo")
}

#[test]
fn routes_prints_the_json_the_list_routes_tool_returns() {
    let out = Command::new(env!("CARGO_BIN_EXE_molde"))
        .args(["routes", "--site"])
        .arg(demo_site())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v, molde::mcp::list_routes(&demo_site()).unwrap());
    assert!(v["total_pages"].as_u64().unwrap() > 0);
    assert!(!v["routes"].as_array().unwrap().is_empty());
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn molde(args: &[&std::ffi::OsStr]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_molde"))
        .args(args)
        .output()
        .unwrap()
}

/// A one-page site whose only page lacks the `<title>` its route promises.
fn failing_site(dir: &std::path::Path) -> std::path::PathBuf {
    let site = dir.join("site");
    std::fs::create_dir_all(site.join("templates")).unwrap();
    std::fs::write(
        site.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: i.j2\n    title: Home\n",
    )
    .unwrap();
    std::fs::write(site.join("data.json"), "{}").unwrap();
    std::fs::write(site.join("templates/i.j2"), "<html><body>x</body></html>").unwrap();
    site
}

#[test]
fn exit_codes_follow_the_documented_contract() {
    use std::ffi::OsStr;
    let dir = tmp("exit");
    let out = dir.join("out");

    // a passing check exits 0 and prints a pass report
    let pass = molde(&[
        "check".as_ref(),
        "--site".as_ref(),
        demo_site().as_os_str(),
        "--out".as_ref(),
        out.as_os_str(),
    ]);
    assert_eq!(pass.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&pass.stdout).unwrap();
    assert_eq!(report["verdict"], "pass");

    // a failing verdict exits 1, and the report still goes to stdout
    let site = failing_site(&dir);
    let fail = molde(&[
        "check".as_ref(),
        "--site".as_ref(),
        site.as_os_str(),
        "--out".as_ref(),
        dir.join("out-fail").as_os_str(),
    ]);
    assert_eq!(fail.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&fail.stdout).unwrap();
    assert_eq!(report["verdict"], "fail");

    // a command that cannot do its job exits 1 (here: no site at that path)
    let missing = dir.join("nothing-here");
    let broken = molde(&[
        "build".as_ref(),
        "--site".as_ref(),
        missing.as_os_str(),
        "--out".as_ref(),
        dir.join("out-b").as_os_str(),
    ]);
    assert_eq!(broken.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&broken.stderr).contains("not a molde site"));

    // a wrong command line exits 2: no command, an unknown command, an unknown flag,
    // a flag without its value, a missing required argument
    let bad: [&[&str]; 5] = [
        &[],
        &["frobnicate"],
        &["check", "--nope"],
        &["check", "--site"],
        &["render"],
    ];
    for args in bad {
        let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        let o = molde(&args);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(o.stdout.is_empty(), "{args:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A running `molde mcp`: lines in on stdin, lines out through a reader thread so a
/// hung server fails the test instead of hanging it.
struct McpProcess {
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
    lines: std::sync::mpsc::Receiver<String>,
}

impl McpProcess {
    fn start(site: &std::path::Path, out: &std::path::Path) -> Self {
        use std::io::BufRead;
        let mut child = Command::new(env!("CARGO_BIN_EXE_molde"))
            .args(["mcp", "--site"])
            .arg(site)
            .arg("--out")
            .arg(out)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                if tx.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        McpProcess {
            child,
            stdin,
            lines,
        }
    }

    fn send(&mut self, line: &str) {
        use std::io::Write;
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
    }

    fn reply(&self) -> serde_json::Value {
        let line = self
            .lines
            .recv_timeout(std::time::Duration::from_secs(60))
            .expect("molde mcp did not answer");
        serde_json::from_str(&line).unwrap()
    }

    fn call(&mut self, request: serde_json::Value) -> serde_json::Value {
        self.send(&request.to_string());
        self.reply()
    }
}

#[test]
fn mcp_speaks_json_rpc_over_real_stdio() {
    use serde_json::json;
    let dir = tmp("mcp");
    let mut mcp = McpProcess::start(&demo_site(), &dir.join("out"));

    let init = mcp.call(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": { "name": "test", "version": "0" } },
    }));
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["serverInfo"]["name"], "molde");
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");

    // a notification has no id and gets no answer: the next reply is the next request's
    mcp.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    let list = mcp.call(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    assert_eq!(list["id"], 2);
    let names: Vec<&str> = list["result"]["tools"]
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

    let routes = mcp.call(json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": { "name": "list_routes", "arguments": {} },
    }));
    assert_eq!(routes["id"], 3);
    assert_eq!(routes["result"]["isError"], false);
    let text = routes["result"]["content"][0]["text"].as_str().unwrap();
    let body: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(body, molde::mcp::list_routes(&demo_site()).unwrap());

    // a line that is not JSON is a parse error with a null id, and the server goes on
    mcp.send("this is not json");
    let bad = mcp.reply();
    assert_eq!(bad["error"]["code"], -32700);
    assert!(bad["id"].is_null());

    let unknown = mcp.call(json!({ "jsonrpc": "2.0", "id": 4, "method": "nope" }));
    assert_eq!(unknown["error"]["code"], -32601);

    // closing stdin ends the session cleanly, with exit code 0
    mcp.stdin = None;
    let status = mcp.child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}
