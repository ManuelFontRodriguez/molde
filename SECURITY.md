# Security policy

## Reporting a vulnerability

Please report security problems privately through GitHub: on this repository's
**Security** tab, choose **Report a vulnerability**. Do not open a public issue for
them.

Include what you found, the molde version (`molde --version`) or commit, and the
smallest site or command that shows it.

I will acknowledge a report within a week, keep you informed, and publish a fix and a
GitHub security advisory before giving details in public. Please give me up to 90 days
before disclosing it yourself; I will credit you in the advisory unless you prefer not.

## Supported versions

Fixes go into the latest release (0.1.x today). Older versions do not get backports
while molde is at 0.x.

## Scope

What molde trusts, and what its agent server lets an agent reach, is described in the
[security model](docs/reference.md#security-model). In short: the site folder is
trusted code; `data.json` and every argument an agent passes to `molde mcp` are not.
A way for either to read or write outside `--site` and `--out`, or to run code, is a
vulnerability. `molde serve` and `molde events` are development servers for
localhost: `serve` deliberately serves any folder it is pointed at, and problems that
need either exposed to a network are not in scope. A way for a web page in the
browser to drive or read them (they check `Host` and `Origin` for that) is in scope.
