# The agent-cost benchmark images. Two targets:
#
#   agent  what the benchmarked agent runs in: toolchains at the build benchmark's
#          pinned versions, pre-installed npm dependencies, Claude Code pinned, a
#          non-root user. NO benchmark material (oracle, golden, tasks, other ports)
#          and NO molde: the preparation step copies molde into the run's context
#          folder only for the molde arms (runner/prep.py, review 2 I6).
#   judge  the agent image plus molde, lychee and Playwright's Chromium. The oracle
#          and the answers are mounted at run time, read-only, never baked in.
#
# Built by ./build.sh from `git archive HEAD` (only committed files). No secret is ever
# written into an image: the Claude Code token arrives at run time as an env variable.

ARG RUST_VERSION=1.89
ARG NODE_VERSION=22.15.0

# --- molde, from the committed source --------------------------------------
FROM rust:${RUST_VERSION}-bookworm AS molde
WORKDIR /src
COPY . .
RUN cargo build --release --locked && ./target/release/molde --version > /molde-version.txt

# --- Node, copied from the official image ---------------------------------
FROM node:${NODE_VERSION}-bookworm-slim AS node

# --- the agent image ------------------------------------------------------
FROM debian:bookworm-slim AS agent
ARG TARGETARCH
ARG HUGO_VERSION=0.168.0
ARG ZOLA_VERSION=0.23.6
ARG CLAUDE_CODE_VERSION=2.1.300

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl git python3 jq procps \
 && rm -rf /var/lib/apt/lists/*

COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=node /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -s /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
 && ln -s /usr/local/lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx

# Hugo: the macOS run used "extended + withdeploy", which has no arm64 Linux build;
# "extended" differs only in the deploy command, which no task uses.
RUN case "$TARGETARCH" in arm64) H=linux-arm64; Z=aarch64-unknown-linux-gnu ;; \
                          amd64) H=linux-amd64; Z=x86_64-unknown-linux-gnu ;; \
                          *) echo "unsupported arch $TARGETARCH"; exit 1 ;; esac \
 && curl -fsSL "https://github.com/gohugoio/hugo/releases/download/v${HUGO_VERSION}/hugo_extended_${HUGO_VERSION}_${H}.tar.gz" \
    | tar -xz -C /usr/local/bin hugo \
 && curl -fsSL "https://github.com/getzola/zola/releases/download/v${ZOLA_VERSION}/zola-v${ZOLA_VERSION}-${Z}.tar.gz" \
    | tar -xz -C /usr/local/bin zola

RUN npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}" && npm cache clean --force

# npm dependency sets, installed once from vendored lockfiles and owned by root (the
# bench user can't change them). Each workspace gets its own writable copy (Astro's Vite
# writes a cache inside node_modules, and Next.js's Turbopack refuses a node_modules
# symlink that leaves the project), so installing is never part of an agent's work, and
# agents have no internet to install anything else.
#   parity-*     the build benchmark's byte-parity ports
#   idiomatic-*  the normal-style ports (adds @astrojs/sitemap; TypeScript for Next.js)
COPY docs/bench/cross-framework/harness/sites/eleventy/package.json docs/bench/cross-framework/harness/sites/eleventy/package-lock.json /opt/node-deps/parity-eleventy/
COPY docs/bench/cross-framework/harness/sites/astro/package.json    docs/bench/cross-framework/harness/sites/astro/package-lock.json    /opt/node-deps/parity-astro/
COPY docs/bench/cross-framework/harness/sites/nextjs/package.json   docs/bench/cross-framework/harness/sites/nextjs/package-lock.json   /opt/node-deps/parity-nextjs/
COPY docs/bench/agent-cost/ports-idiomatic/astro/package.json  docs/bench/agent-cost/ports-idiomatic/astro/package-lock.json  /opt/node-deps/idiomatic-astro/
COPY docs/bench/agent-cost/ports-idiomatic/nextjs/package.json docs/bench/agent-cost/ports-idiomatic/nextjs/package-lock.json /opt/node-deps/idiomatic-nextjs/
RUN for d in /opt/node-deps/*/; do (cd "$d" && npm ci --silent --no-audit --no-fund) || exit 1; done \
 && npm cache clean --force

# Every version, recorded in the image itself.
# No molde line here: the agent image is shared by every arm, and the non-molde arms
# (and the port-factory agents) must see nothing of molde. The judge adds it below.
RUN mkdir -p /opt/versions && { echo "hugo: $(hugo version)"; echo "zola: $(zola --version)"; \
      echo "node: $(node --version)"; echo "claude: $(claude --version)"; \
      for d in /opt/node-deps/*/; do echo "$(basename $d): $(cd $d && node -e 'const l=require("./package-lock.json").packages[""]; console.log(JSON.stringify({...l.dependencies,...l.devDependencies}))')"; done; \
    } > /opt/versions/all.txt && cat /opt/versions/all.txt

RUN useradd --create-home --uid 1000 bench && mkdir -p /work && chown bench /work
USER bench
WORKDIR /work
# Claude Code 2.1.300 ships two built-in plugins, enabled by default. One loads AGENTS.md
# files as project instructions, which would give an arm context outside its arm.md; the
# metrics flag any loaded plugin as infrastructure. Disabled for the bench user, and
# checked: the init event then lists no plugins.
RUN claude plugin disable cc-plugin-agents-md@builtin -s user \
 && claude plugin disable cc-plugin-plugin-authoring@builtin -s user
ENV PATH=/usr/local/bin:/usr/bin:/bin \
    DISABLE_AUTOUPDATER=1 \
    CLAUDE_CODE_DISABLE_AUTO_MEMORY=1 \
    CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 \
    NEXT_TELEMETRY_DISABLED=1 \
    ASTRO_TELEMETRY_DISABLED=1

# --- the judge image ------------------------------------------------------
FROM agent AS judge
ARG TARGETARCH
ARG LYCHEE_VERSION=0.24.2
ARG PLAYWRIGHT_VERSION=1.63.0
USER root
COPY --from=molde /src/target/release/molde /opt/molde-real/molde
COPY --from=molde /molde-version.txt /opt/versions/molde.txt
COPY BENCH_SOURCE_REV /opt/versions/source-rev.txt
RUN echo "molde: $(cat /opt/versions/molde.txt) (source $(cat /opt/versions/source-rev.txt); copied in per run for molde arms only)" >> /opt/versions/all.txt
RUN case "$TARGETARCH" in arm64) L=aarch64-unknown-linux-gnu ;; amd64) L=x86_64-unknown-linux-gnu ;; esac \
 && curl -fsSL "https://github.com/lycheeverse/lychee/releases/download/lychee-v${LYCHEE_VERSION}/lychee-${L}.tar.gz" \
    | tar -xz -C /usr/local/bin --strip-components=1 "lychee-${L}/lychee" \
 && ln -s /opt/molde-real/molde /usr/local/bin/molde \
 && echo "lychee: $(lychee --version)" >> /opt/versions/all.txt
# Playwright + Chromium for the browser checks (T8, T9): judge image only.
RUN mkdir -p /opt/playwright && cd /opt/playwright \
 && npm init -y >/dev/null && npm install --silent --no-audit --no-fund "playwright@${PLAYWRIGHT_VERSION}" \
 && PLAYWRIGHT_BROWSERS_PATH=/opt/playwright/browsers npx playwright install --with-deps chromium \
 && rm -rf /var/lib/apt/lists/* && npm cache clean --force \
 && echo "playwright: ${PLAYWRIGHT_VERSION} ($(ls /opt/playwright/browsers))" >> /opt/versions/all.txt
USER bench
