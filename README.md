# 模型智力检测

[![CI](https://github.com/shiceai/codex-proxy-intelligence-check/actions/workflows/ci.yml/badge.svg)](https://github.com/shiceai/codex-proxy-intelligence-check/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/shiceai/codex-proxy-intelligence-check?sort=semver&style=flat-square)](https://github.com/shiceai/codex-proxy-intelligence-check/releases)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=flat-square)](LICENSE)

简体中文 | [English](README.en.md)

[Codex Proxy RS](https://github.com/zyycn/codex-proxy-rs) 的插件，用来检查号池里的 OpenAI 账号有没有被降智。

做法很简单：拿一道有标准答案的推理题去问选中的账号，答对记为正常，答错记为未通过，每个账号保留最近 20 次结果。内置的是社区里常用的“糖果题”，也可以自己加题。

插件只注册管理页面和后台维护两项能力，不介入请求链路，也不会改动账号、分组和 Key。

## 安装

宿主需要 3.37 及以上的 3.x 版本，运行在 Linux x86_64 上（glibc 2.34 以上，官方 Debian bookworm 镜像满足）。

在宿主的「插件管理」点安装，来源选 GitHub，填 `shiceai/codex-proxy-intelligence-check`，标签留空即安装最新版本，以后也能在宿主里检查更新。

也可以从 [Releases](https://github.com/shiceai/codex-proxy-intelligence-check/releases) 下载 `shiceai.intelligence-check-1.0.0-x86_64-unknown-linux-gnu.tar.gz`，用同名的 `.sha256` 核对后，选「上传包」安装。

安装时核对名称、发布者、版本和摘要。装好后插件会自动启用，左侧导航里会出现「模型智力检测」。

> [!IMPORTANT]
> 插件进程和宿主使用同一个系统身份，宿主不做权限隔离。安装前请确认你信任这份代码。

## 使用

先在页面顶部选计量 Key、模型和推理强度。模型默认 `gpt-6-astra`，糖果题主要拿它来测；推理强度默认 high。检测用量记在所选 Key 上，被测账号也必须在这个 Key 的分组里，否则宿主会拒绝请求，结果记为错误。

勾选账号，点「开始检测」，每个账号会把已启用的题依次跑一遍。检测在插件后台进行，关掉或刷新页面不影响，回来时页面会接着显示进度。点「停止」后不再开始新题，正在跑的题会跑完。

账号表里的色块是最近 20 次结果：绿色正常，红色未通过，灰色错误。点色块可以看模型的回答、耗时、token 用量和请求 ID。

糖果题只能启用或停用，不能修改，这样不同时间的结果才能放在一起比。自定义题可以新增、编辑和删除，最多 20 道。

## 判分

只看模型最后给出的回答，也就是 Responses 中 `final_answer` 阶段的 assistant 消息，推理过程不参与判分。回答先做 Unicode NFKC 归一化、去掉首尾空白，整段被代码块包住时剥掉这一层，然后按题目设定的方式比较：

- 完全一致：回答必须正好是标准答案。标准答案是 `21` 时，回答 `答案是21` 也算错
- 包含：回答里出现标准答案就算对

宿主拒绝、上游报错、超过 150 秒、响应没有完成或者没有最终回答，都记为错误。错误不计入通过率，通过率按最近 20 次计算：正常 /（正常 + 未通过）。

检测请求走宿主平常的请求链，模型映射和其他插件的中间件都会生效。测出来的是这个账号在你当前配置下的表现，不完全等于模型本身的能力。

## 后台执行的限制

- 宿主大约每 30 秒调用一次插件的后台处理，新提交的检测最多要等 30 秒左右才开始
- 同时最多跑 3 道题，同一个账号的题按顺序跑，每道题最长 150 秒
- 同一时间只能有一批检测，一批最多 1000 道题（账号数 × 题目数）
- 插件停用、升级或宿主重启时，正在跑的题会被取消并标为“检测被中断”。这些请求可能已经计费，所以不会自动重跑；还没开始的题在插件恢复后继续

目前不支持定时检测、按结果自动停用账号或切换分组、Claude 等其他平台的账号、按耗时判定、正则匹配和导出结果。

## 从源码构建

需要 Rust 1.99.1（由 `rust-toolchain.toml` 自动选用）、Node.js 24 和 pnpm 12.6。pnpm 通过 corepack 调用，要在 `frontend` 目录里执行，corepack 才会用 `package.json` 指定的版本。

```bash
# 后端
cd backend
cargo fmt --check
RUST_MIN_STACK=16777216 cargo clippy --all-targets --locked -- -D warnings
RUST_MIN_STACK=16777216 cargo test --locked

# 前端
cd frontend
corepack pnpm install --frozen-lockfile
corepack pnpm lint && corepack pnpm typecheck && corepack pnpm test && corepack pnpm build
corepack pnpm dev   # 用模拟数据预览页面，不调用真实模型
```

打安装包要先装打包工具 `cpr-plugin`。它和 SDK 都固定在 codex-proxy-rs 的 `8c8618a` 提交（v3.33.0），到 v3.37 为止包格式和插件协议都没有变化。

```bash
cargo install --locked --git https://github.com/zyycn/codex-proxy-rs.git \
  --rev 8c8618aa0f841638262262e3338c115bc075f125 codex-proxy-plugin-cli --root .tools
PLUGIN_CLI="$PWD/.tools/bin/cpr-plugin" bash scripts/package
```

安装包和 `.sha256` 输出到 `dist/`。Releases 里的安装包由 GitHub Actions 按同样的步骤构建。

## 版本记录

- 1.0.0：首个公开版本

## 反馈

本项目由 [Shice AI](https://shice.ai) 维护。问题和建议请提 [Issue](https://github.com/shiceai/codex-proxy-intelligence-check/issues)；安全问题请通过[私密漏洞报告](https://github.com/shiceai/codex-proxy-intelligence-check/security/advisories/new)提交，不要公开发 issue。

## 许可

[Apache-2.0](LICENSE)。工程骨架改自 [codex-proxy-plugins](https://github.com/zyycn/codex-proxy-plugins) 的 workbench 示例，同为 Apache-2.0。
