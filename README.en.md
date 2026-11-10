# Model Intelligence Check

[![CI](https://github.com/shiceai/codex-proxy-intelligence-check/actions/workflows/ci.yml/badge.svg)](https://github.com/shiceai/codex-proxy-intelligence-check/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/shiceai/codex-proxy-intelligence-check?sort=semver&style=flat-square)](https://github.com/shiceai/codex-proxy-intelligence-check/releases)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=flat-square)](LICENSE)

[简体中文](README.md) | English

A [Codex Proxy RS](https://github.com/zyycn/codex-proxy-rs) plugin that checks whether the OpenAI accounts in your pool are being served a degraded model.

It sends a reasoning question with a known answer to each selected account. A correct answer counts as normal, a wrong one as failed, and the last 20 results are kept for each account. A puzzle widely used in the community (the "candy question") is built in, and you can add your own questions.

The plugin only registers a management page and a background maintenance task. It stays out of the request pipeline and never changes accounts, groups or keys. Its UI is in Simplified Chinese.

## Installation

Requires Codex Proxy RS 3.37 or a later 3.x release, running on Linux x86_64 (glibc 2.34 or later; the official Debian bookworm image works).

In the host, open **Plugins**, click install, choose GitHub as the source and enter `shiceai/codex-proxy-intelligence-check`. Leave the tag empty to get the latest release. The host can then check for updates later.

You can also download `shiceai.intelligence-check-1.0.0-x86_64-unknown-linux-gnu.tar.gz` from [Releases](https://github.com/shiceai/codex-proxy-intelligence-check/releases), verify it against the matching `.sha256` file, and install it with **Upload package**.

Check the name, publisher, version and digest before confirming. The plugin is enabled after installation and appears in the sidebar as 模型智力检测.

> [!IMPORTANT]
> The plugin process runs as the same system user as the host, and the host does not sandbox it. Only install it if you trust this code.

## Usage

At the top of the page, pick the key to bill the checks to, a model and a reasoning effort. The model defaults to `gpt-6-astra`, which the candy question is mainly meant for; effort defaults to high. The accounts under test must be within the selected key's groups, otherwise the host rejects the request and the result is recorded as an error.

Select accounts and click 开始检测 (Start). Each account runs every enabled question in turn. Checks run in the plugin's background, so closing or reloading the page doesn't stop them, and the page shows the progress again when you come back. After 停止 (Stop), no new questions start, and questions already running finish.

The colored blocks in the account table are the last 20 results: green for normal, red for failed, grey for error. Click a block to see the model's answer, duration, token usage and request ID.

The candy question can be enabled or disabled but not edited, so results from different times stay comparable. Custom questions can be added, edited and deleted, up to 20 in total.

## Scoring

Only the model's final visible answer counts, that is, the assistant message in the `final_answer` phase of the Responses output. Reasoning is ignored. The answer is NFKC-normalized and trimmed, and a code block wrapping the whole answer is stripped. It is then compared using the question's match mode:

- Exact: the answer must be exactly the expected answer. If the expected answer is `21`, `答案是21` ("the answer is 21") is wrong
- Contains: the answer is correct if it contains the expected answer

Host rejections, upstream errors, requests over 150 seconds, incomplete responses and responses without a final answer are recorded as errors. Errors don't count toward the pass rate, which is normal / (normal + failed) over the last 20 results.

Checks go through the host's normal request pipeline, including model mapping and other plugins' middleware. What you measure is how the account behaves under your current setup, which is not quite the same as the model's own capability.

## Background limits

- The host calls the plugin's background handler about every 30 seconds, so a new check can take up to about 30 seconds to start
- Up to 3 questions run at once. Questions for the same account run one after another, each for at most 150 seconds
- Only one batch can run at a time, with at most 1000 questions (accounts × questions)
- When the plugin is disabled or upgraded, or the host restarts, running questions are cancelled and marked 检测被中断 (interrupted). They are not retried automatically, since the requests may already have been billed. Questions that hadn't started yet continue once the plugin is back

Not supported yet: scheduled checks, pausing accounts or switching groups based on results, accounts from other platforms such as Claude, latency-based verdicts, regex matching and exporting results.

## Building from source

You need Rust 1.99.1 (picked up from `rust-toolchain.toml`), Node.js 24 and pnpm 12.6. Run pnpm through corepack from inside `frontend`, so that corepack uses the version pinned in `package.json`.

```bash
# Backend
cd backend
cargo fmt --check
RUST_MIN_STACK=16777216 cargo clippy --all-targets --locked -- -D warnings
RUST_MIN_STACK=16777216 cargo test --locked

# Frontend
cd frontend
corepack pnpm install --frozen-lockfile
corepack pnpm lint && corepack pnpm typecheck && corepack pnpm test && corepack pnpm build
corepack pnpm dev   # preview the page with mock data, no real model calls
```

Packaging needs the `cpr-plugin` tool. Like the SDK, it is pinned to commit `8c8618a` of codex-proxy-rs (v3.33.0). The package format and plugin protocol are unchanged through v3.37.

```bash
cargo install --locked --git https://github.com/zyycn/codex-proxy-rs.git \
  --rev 8c8618aa0f841638262262e3338c115bc075f125 codex-proxy-plugin-cli --root .tools
PLUGIN_CLI="$PWD/.tools/bin/cpr-plugin" bash scripts/package
```

The package and its `.sha256` are written to `dist/`. Packages on the Releases page are built by GitHub Actions with the same steps.

## Changelog

- 1.0.0: first public release

## Feedback

Maintained by [Shice AI](https://shice.ai). Please report bugs and suggestions in [Issues](https://github.com/shiceai/codex-proxy-intelligence-check/issues). For security issues, use [private vulnerability reporting](https://github.com/shiceai/codex-proxy-intelligence-check/security/advisories/new) instead of opening a public issue.

## License

[Apache-2.0](LICENSE). The project skeleton is adapted from the workbench example in [codex-proxy-plugins](https://github.com/zyycn/codex-proxy-plugins), also licensed under Apache-2.0.
