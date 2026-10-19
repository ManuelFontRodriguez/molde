#!/usr/bin/env python3
"""Turn one run folder into metrics.json (DESIGN.md §7):

    metrics.py RESULTS/<set>/<run-id>

Reads transcript.jsonl (Claude Code stream-json), verdict.json (the judge), meta.json and
manifest.json. Token totals come only from the final `result` event; per-message
usage is never summed (it repeats across the content blocks of one message: review 1 I3).

Outcome classes:
  infra_fail  auth error, API error, rate-limit exhaustion, no result event, or a run
              whose environment was not the intended one (wrong model, extra tools or MCP
              servers): re-run, never counted against an arm
  censored    hit the turn limit or the time limit
  pass/fail   the judge's verdict
claimed_done (structural): the run ended with subtype "success", is_error false, and a
non-empty final message. claimed_done_lexical additionally requires the final message not
to say it was blocked or unable (BLOCKED_PATTERNS); a sensitivity analysis only.
false_done = claimed_done and not pass; its kind comes from the LAST result of the agent's
own checker (molde check / site-check / the MCP check tool) before it stopped:
checker_failed_ignored, checker_passed_blind, checker_no_report or no_checker_run.
"""
import json
import re
import sys
from pathlib import Path

BLOCKED_PATTERNS = re.compile(
    r"\b(i (was|am) (not able|unable)|could not|couldn't|can ?not|cannot|unable to|blocked|"
    r"did not manage|didn't manage|not possible|i (did|have) not (finish|complete)|incomplete)\b", re.I)
EXPECTED_TOOLS = {"Bash", "Read", "Edit", "Write"}
# Applied only to error results: a normal final message may well say "rate limit" or "429".
RATE_LIMIT = re.compile(r"(?i)(usage limit|rate limit|rate_limit|overloaded|\b429\b|too many requests|limit reached|hit your limit)")
# A usage-limit notice that comes back as the "final message" of a non-error result: short
# and nothing else. A real summary that mentions a rate limit is long or lacks the reset.
LIMIT_NOTICE = re.compile(r"(?i)(usage limit|hit your limit|limit reached).{0,120}(reset|try again)|limit will reset")
CENSOR_SUBTYPES = ("error_max_turns", "error_max_budget_usd")   # caps: censored, never re-run
MIN_PROGRESS_TURNS = 3
DOCS_FINGERPRINT = "Everything a site author"   # the molde reference's first sentence
SAVED_TO = re.compile(r"saved to: (/[^\s\\'\"]+/tool-results/[^\s\\'\"]+)")  # a persisted large output
# A shell command that changes files (agents often edit with sed -i or a heredoc instead of
# the Edit tool). Redirections to /dev/null and to stderr don't count.
BASH_EDIT = re.compile(r"(\bsed\s+(-[a-zA-Z]*i|--in-place)|\bperl\s+-[a-zA-Z]*i|\btee\b|\bpatch\b|\bgit\s+(apply|checkout|restore)\b"
                       r"|\b(mv|cp|rm|mkdir|touch)\s|(^|\s)>{1,2}\s*(?!/dev/null)[\w./$~-])")   # a timed-out run with fewer assistant messages made no progress: infra
SECRET = re.compile(r"sk-ant-[A-Za-z0-9_-]{8,}")


def load_events(path):
    events, bad = [], 0
    for line in path.read_text(errors="replace").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            events.append(json.loads(line))
        except ValueError:
            bad += 1
    return events, bad


def phase_of(name, inp, build_cmd):
    """Classify a tool call: docs, explore, edit, verify (review 1 I1, review 2 I14).
    verify = running the arm's own build command or a checker; docs = reading the molde
    reference; everything else that isn't an edit is explore."""
    if name in ("Edit", "Write"):
        return "edit"
    if name.startswith("mcp__"):
        return "verify"
    if name == "Read":
        return "docs" if "molde-reference.md" in inp.get("file_path", "") else "explore"
    if name == "Bash":
        c = inp.get("command", "")
        if "molde-reference.md" in c:
            return "docs"
        first = build_cmd.split("&&")[-1].strip().split(" ")[:2]
        if re.search(r"(^|[\s;&|(])(molde\s+(check|build)|site-check)\b", c) or (first and " ".join(first) in c):
            return "verify"
        if BASH_EDIT.search(c):
            return "edit"
    return "explore"


def main():
    run = Path(sys.argv[1])
    meta = json.loads((run / "meta.json").read_text())
    manifest = json.loads((run / "manifest.json").read_text())
    verdict = json.loads((run / "verdict.json").read_text()) if (run / "verdict.json").exists() else None
    if verdict and verdict.get("judge_error"):
        verdict = None
        judge_error = True
    else:
        judge_error = False
    m = {"run_id": meta["run_id"], "arm": meta["arm"], "task": meta["task"], "prompt": meta["prompt"],
         "rep": meta["rep"], "set": meta["set"], "model_requested": meta["model"], "effort": meta["effort"],
         "planted": manifest.get("planted", []), "pass": verdict["pass"] if verdict else None,
         "oracle_pass": verdict["oracle"]["pass"] if verdict else None,
         "mutation_pass": verdict["mutation"]["oracle"]["pass"] if verdict else None,
         "problems": [], "infra_reasons": ["judge error (harness): see judge.log"] if judge_error else []}
    if meta.get("dry_run"):
        m["outcome"] = "dry_run" if not judge_error else "infra_fail"
        (run / "metrics.json").write_text(json.dumps(m, indent=1))
        print(json.dumps({k: m[k] for k in ("run_id", "outcome", "pass")}))
        return
    tpath = run / "transcript.jsonl"
    raw = tpath.read_text(errors="replace") if tpath.exists() else ""
    stderr = (run / "agent.stderr").read_text(errors="replace") if (run / "agent.stderr").exists() else ""
    events, bad = load_events(tpath) if tpath.exists() else ([], 0)
    m["unparsed_lines"] = bad
    # run_one.py scrubs token-shaped strings from every saved file before this runs, and
    # lists the files it changed; either signal marks the run (review 2 I7).
    m["token_scrubbed_files"] = meta.get("token_scrubbed_files") or []
    m["secret_in_transcript"] = bool(SECRET.search(raw) or SECRET.search(stderr) or m["token_scrubbed_files"])

    init = next((e for e in events if e.get("type") == "system" and e.get("subtype") == "init"), None)
    result = next((e for e in reversed(events) if e.get("type") == "result"), None)
    if init:
        tools = set(init.get("tools", []))
        mcp_tools = {t for t in tools if t.startswith("mcp__")}
        m["init"] = {"tools": sorted(tools), "mcp_servers": init.get("mcp_servers"), "model": init.get("model"),
                     "permissionMode": init.get("permissionMode"), "apiKeySource": init.get("apiKeySource"),
                     "claude_code_version": init.get("claude_code_version"),
                     "plugins": init.get("plugins"), "skills": init.get("skills"), "slash_commands": init.get("slash_commands")}
        if tools - mcp_tools != EXPECTED_TOOLS:
            m["infra_reasons"].append(f"unexpected built-in tools: {sorted(tools - mcp_tools)}")
        want_mcp = "mcp.json" in manifest["files"]
        servers = init.get("mcp_servers") or []
        if want_mcp and not any(s.get("name") == "molde" and s.get("status") == "connected" for s in servers):
            m["infra_reasons"].append(f"molde MCP server not connected: {servers}")
        if not want_mcp and (servers or mcp_tools):
            m["infra_reasons"].append(f"MCP present in a non-MCP arm: {servers}")
        if init.get("plugins"):
            m["infra_reasons"].append(f"plugins loaded: {init.get('plugins')}")
    else:
        m["infra_reasons"].append("no init event")

    # tool calls, deduplicated by tool_use id
    # The subscription's own usage windows, as Claude Code reports them (rate_limit_event):
    # the campaign runner paces on these and stops before a window is exhausted.
    rl = [e.get("rate_limit_info") for e in events if e.get("type") == "rate_limit_event" and e.get("rate_limit_info")]
    m["rate_limit_last"] = rl[-1] if rl else None
    m["assistant_messages"] = len({(e.get("message") or {}).get("id") for e in events if e.get("type") == "assistant"} - {None})
    calls, seen = [], set()
    for e in events:
        if e.get("type") != "assistant":
            continue
        for b in (e.get("message") or {}).get("content", []) or []:
            if isinstance(b, dict) and b.get("type") == "tool_use" and b.get("id") not in seen:
                seen.add(b.get("id"))
                calls.append((b.get("name"), b.get("input") or {}))
    m["tool_calls"] = len(calls)
    by = {}
    phases = {"docs": 0, "explore": 0, "edit": 0, "verify": 0}
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from arms import ARMS
    build_cmd = ARMS[meta["arm"]]["note"]["build"]
    for name, inp in calls:
        by[name] = by.get(name, 0) + 1
        phases[phase_of(name, inp, build_cmd)] += 1
    m["tool_calls_by_name"] = by
    m["tool_calls_by_phase"] = phases
    # Tokens spent reading the docs (DESIGN §2: every headline also without them). Each
    # tool result enters the context as cache creation in the next assistant message; a
    # docs result's share of that message's new input, by characters, is attributed to the
    # docs. An estimate (exact only when a turn's results are all docs or all not).
    docs_ids = set()
    for e in events:
        if e.get("type") == "assistant":
            for b in (e.get("message") or {}).get("content", []) or []:
                if isinstance(b, dict) and b.get("type") == "tool_use" and \
                        phase_of(b.get("name", ""), b.get("input") or {}, build_cmd) == "docs":
                    docs_ids.add(b.get("id"))
    # A docs read can also arrive under another name: a large `cat` of the reference is
    # saved by Claude Code to a tool-results file, which the agent then Reads. Such a result
    # is recognised by the reference's own opening text, and counted as docs. Any later call
    # that names that saved file (a `sed` or `grep` of sections of it) reads the docs too.
    saved, phase_by_id = set(), {}
    for e in events:
        if e.get("type") == "assistant":
            for b in (e.get("message") or {}).get("content", []) or []:
                if isinstance(b, dict) and b.get("type") == "tool_use":
                    inp = b.get("input") or {}
                    phase_by_id[b.get("id")] = phase_of(b.get("name", ""), inp, build_cmd)
                    if b.get("id") not in docs_ids and any(p in json.dumps(inp) for p in saved):
                        docs_ids.add(b.get("id"))
                        phases[phase_by_id[b.get("id")]] -= 1
                        phases["docs"] += 1
            continue
        if e.get("type") != "user":
            continue
        for b in (e.get("message") or {}).get("content", []) or []:
            if isinstance(b, dict) and b.get("type") == "tool_result":
                c = b.get("content")
                c = c if isinstance(c, str) else json.dumps(c)
                if DOCS_FINGERPRINT not in c:
                    continue
                saved.update(SAVED_TO.findall(c))
                if b.get("tool_use_id") not in docs_ids:
                    docs_ids.add(b.get("tool_use_id"))
                    phases[phase_by_id.get(b.get("tool_use_id"), "explore")] -= 1
                    phases["docs"] += 1
    m["tool_calls_by_phase"] = phases
    pending, docs_est, seen_msgs = None, 0.0, set()
    for e in events:
        if e.get("type") == "user":
            tot = doc = 0
            for b in (e.get("message") or {}).get("content", []) or []:
                if isinstance(b, dict) and b.get("type") == "tool_result":
                    c = b.get("content")
                    n = len(c if isinstance(c, str) else json.dumps(c))
                    tot += n
                    doc += n if b.get("tool_use_id") in docs_ids else 0
            if tot:
                pending = (pending[0] + doc, pending[1] + tot) if pending else (doc, tot)
        elif e.get("type") == "assistant" and pending:
            msg = e.get("message") or {}
            u = msg.get("usage") or {}
            if msg.get("id") in seen_msgs or not u:
                continue
            seen_msgs.add(msg.get("id"))
            new_in = (u.get("cache_creation_input_tokens") or 0) + (u.get("input_tokens") or 0)
            if pending[0]:
                docs_est += new_in * pending[0] / pending[1]
            pending = None
    m["docs_tokens_est"] = round(docs_est)
    cmds = [inp.get("command", "") for n, inp in calls if n == "Bash"]
    m["forbidden_tool_use"] = []
    if meta["arm"] in ("molde-build", "molde+checker"):
        m["forbidden_tool_use"] = [c for c in cmds if ".molde" in c or re.search(r"\bmolde (check|mcp|events)\b", c)]
    m["shim_refusals"] = sum(1 for t in raw.split("\n") if "is not available in this environment" in t)
    m["bash_commands"] = cmds
    # edits made through the shell, wherever the call was classed (one call can edit and verify)
    m["bash_edit_calls"] = sum(1 for c in cmds if BASH_EDIT.search(c))

    # The agent's own checker (B3): only results of actual checker calls count (molde
    # check, site-check, the MCP check tool), matched by tool_use id; reading the docs
    # (which contain an example failing report) is not a signal. The last verdict before
    # the agent stopped decides the false-done kind.
    checker_ids = {}
    for e in events:
        if e.get("type") != "assistant":
            continue
        for b in (e.get("message") or {}).get("content", []) or []:
            if not (isinstance(b, dict) and b.get("type") == "tool_use"):
                continue
            n, inp = b.get("name", ""), b.get("input") or {}
            if (n == "Bash" and re.search(r"(^|[\s;&|(])(molde\s+check|site-check)\b", inp.get("command", ""))) \
                    or n == "mcp__molde__check":
                checker_ids[b.get("id")] = n
    verdicts = []
    bash_timeouts = 0
    for e in events:
        if e.get("type") != "user":
            continue
        for b in (e.get("message") or {}).get("content", []) or []:
            if not (isinstance(b, dict) and b.get("type") == "tool_result"):
                continue
            c = b.get("content")
            text = c if isinstance(c, str) else json.dumps(c)
            if re.search(r"timed out|Command timed out", text) and "molde" not in text[:0]:
                bash_timeouts += bool(re.search(r"(?i)command timed out after", text))
            if b.get("tool_use_id") in checker_ids:
                vm = re.search(r'\\?"verdict\\?"\s*:\s*\\?"(pass|fail)', text)
                verdicts.append(vm.group(1) if vm else "no_report")
    m["checker_calls"] = len(checker_ids)
    m["checker_verdicts"] = verdicts
    m["signal_ever_fail"] = "fail" in verdicts
    m["signal_last"] = verdicts[-1] if verdicts else None
    m["bash_timeouts"] = bash_timeouts

    if result:
        u = result.get("usage") or {}
        m["result"] = {k: result.get(k) for k in ("subtype", "is_error", "num_turns", "duration_ms", "duration_api_ms",
                                                   "total_cost_usd", "terminal_reason", "api_error_status")}
        m["usage"] = {k: u.get(k) for k in ("input_tokens", "output_tokens", "cache_creation_input_tokens",
                                            "cache_read_input_tokens")}
        m["usage"]["thinking_tokens"] = (u.get("output_tokens_details") or {}).get("thinking_tokens")
        m["new_tokens"] = sum((u.get(k) or 0) for k in ("input_tokens", "cache_creation_input_tokens", "output_tokens"))
        m["context_tokens_processed"] = m["new_tokens"] + (u.get("cache_read_input_tokens") or 0)
        mu = result.get("modelUsage") or {}
        m["models_used"] = sorted(mu)
        m["model_usage"] = mu
        pinned = [v for k, v in mu.items() if k.startswith(meta["model"])]
        if pinned:
            pu = pinned[0]
            g = lambda *ks: next((pu[k] for k in ks if k in pu), 0) or 0
            m["new_tokens_pinned_model"] = (g("inputTokens", "input_tokens") + g("cacheCreationInputTokens", "cache_creation_input_tokens")
                                            + g("outputTokens", "output_tokens"))
            m["cache_read_pinned_model"] = g("cacheReadInputTokens", "cache_read_input_tokens")
        main_models = [k for k in mu if not re.search(r"haiku", k, re.I)]
        if any(not k.startswith(meta["model"]) for k in main_models):
            m["infra_reasons"].append(f"model other than {meta['model']} used: {sorted(mu)}")
        final = result.get("result") or ""
        m["final_message"] = final[-2000:]
        # Review 2 I1: any error result that is not the turn limit is infrastructure (auth,
        # API, rate or usage limit, overload), re-run and never counted against an arm.
        if result.get("subtype") not in CENSOR_SUBTYPES and (
                result.get("is_error") or result.get("subtype") != "success"
                or result.get("api_error_status") or result.get("terminal_reason") == "api_error"
                or (len(final.strip()) < 300 and LIMIT_NOTICE.search(final))):
            m["infra_reasons"].append(f"error result ({result.get('subtype')}, {result.get('terminal_reason')}, "
                                      f"status {result.get('api_error_status')}): {final[:200]}")
            m["limit_hit"] = bool(result.get("api_error_status") == 429 or RATE_LIMIT.search(final[-600:]))
        censored = result.get("subtype") in CENSOR_SUBTYPES or meta.get("agent_exit") == 124
        # Review 2 I2: structural, as DESIGN.md says. The lexical version is kept only as a
        # sensitivity analysis (claimed_done_lexical).
        claimed = result.get("subtype") == "success" and not result.get("is_error") and bool(final.strip())
        m["claimed_done_lexical"] = claimed and not BLOCKED_PATTERNS.search(final[-1500:])
    else:
        m["infra_reasons"].append("no result event" + (" (timed out)" if meta.get("agent_exit") == 124 else ""))
        censored = meta.get("agent_exit") == 124
        claimed = False
        # No result event: estimate tokens from the assistant messages (the last usage seen
        # per message id, since usage repeats across a message's content blocks), so the
        # heaviest runs don't drop out of the cost totals. Flagged as an estimate.
        per_msg = {}
        for e in events:
            msg = e.get("message") or {}
            if e.get("type") == "assistant" and msg.get("id") and msg.get("usage"):
                per_msg[msg["id"]] = msg["usage"]
        if per_msg:
            keys = ("input_tokens", "cache_creation_input_tokens", "output_tokens", "cache_read_input_tokens")
            tot = {k: sum((u.get(k) or 0) for u in per_msg.values()) for k in keys}
            m["usage"] = tot
            m["new_tokens"] = tot["input_tokens"] + tot["cache_creation_input_tokens"] + tot["output_tokens"]
            m["context_tokens_processed"] = m["new_tokens"] + tot["cache_read_input_tokens"]
            m["tokens_estimated"] = True
        # A timeout with (almost) no assistant messages is a stuck API or network, not the
        # arm working: infrastructure, re-run (not censored).
        if censored and m["assistant_messages"] < MIN_PROGRESS_TURNS:
            censored = False
            m["infra_reasons"].append(f"no progress before the timeout ({m['assistant_messages']} assistant messages)")
    m["claimed_done"] = claimed
    # Review 2 I1: an out-of-memory kill (exit 137) is the arm's own cost, not infrastructure:
    # it counts as a failed attempt, flagged.
    # Docker's own OOMKilled flag decides (run_one records it); older runs without it fall
    # back to exit 137. A kill that wasn't OOM (container removed, host stop) is infrastructure.
    m["oom"] = meta["oom_killed"] if "oom_killed" in meta else meta.get("agent_exit") == 137
    if meta.get("agent_exit") == 137 and not m["oom"]:
        m["infra_reasons"].append("agent killed from outside (exit 137, not out of memory)")
    if m["oom"]:
        m["outcome"] = "fail"
        m["infra_reasons"] = [r for r in m["infra_reasons"] if not r.startswith("no result event")]
    elif m["infra_reasons"] and not (censored and not result):
        m["outcome"] = "infra_fail"
    elif censored:
        m["outcome"] = "censored"
    else:
        m["outcome"] = "pass" if m["pass"] else "fail"
    m["false_done"] = bool(claimed and m["outcome"] == "fail")
    if m.get("new_tokens") is not None:
        m["new_tokens_without_docs"] = max(0, m["new_tokens"] - m["docs_tokens_est"])
    # the three kinds (DESIGN.md §7, refined after review 2 B3)
    m["false_done_kind"] = None
    if m["false_done"]:
        m["false_done_kind"] = {"fail": "checker_failed_ignored", "pass": "checker_passed_blind",
                                "no_report": "checker_no_report"}.get(m["signal_last"], "no_checker_run")
    ds = run / "diff-stat.txt"
    if ds.exists():
        last = (ds.read_text().strip().splitlines() or [""])[-1]
        nums = [int(x) for x in re.findall(r"(\d+) (?:files? changed|insertions?|deletions?)", last)]
        m["diff_files_changed"] = nums[0] if nums else 0
        m["diff_lines"] = sum(nums[1:]) if len(nums) > 1 else 0
    if m["secret_in_transcript"]:
        m["problems"].append("token-shaped string found (scrubbed from: "
                             f"{', '.join(m['token_scrubbed_files']) or 'not scrubbed'}): check before publishing")
    # An attempt the shim refused is the arm working as designed; only a forbidden call that
    # wasn't refused (it reached the real molde) is a problem.
    m["forbidden_tool_refused"] = min(len(m["forbidden_tool_use"]), m["shim_refusals"])
    if len(m["forbidden_tool_use"]) > m["shim_refusals"]:
        m["problems"].append("forbidden tool use not refused by the shim")
    m["wall_s"] = meta.get("agent_wall_s")
    (run / "metrics.json").write_text(json.dumps(m, indent=1, ensure_ascii=False))
    print(json.dumps({k: m.get(k) for k in ("run_id", "outcome", "pass", "claimed_done", "new_tokens", "tool_calls",
                                            "infra_reasons", "problems")}))


if __name__ == "__main__":
    main()
