# Task: recover real Rust API + logic for hydra_protocol and hydra_lich

Context: ~/Code/hydra-recon is a stub workspace generated from the symbol table of the Hydra GemStone IV
front end (arm64 Mach-O at ~/Code/hydra-inspect/hydra, demangled symbols in ~/Code/hydra-inspect/symbols_demangled.txt).
Every stub fn is "pub fn name() { todo!() }" with its binary address in a "/// @0x..." comment (addresses are
unslid virtual addresses; the function ends where the next symbol begins). Local research use only; nothing is published.

## Orchestration
You are the parent. Use your reasonix — a config- and plugin-driven coding agent (multi-model)

Usage:
  reasonix [--model NAME] [-c|--continue] [-r|--resume [QUERY]] [--permission-mode MODE] [--effort LEVEL] [--add-dir PATH]   interactive session
  reasonix -p|--print [--model NAME] [--output-format text|json|stream-json] [--allowed-tools RULES] [--add-dir PATH] <task>
  reasonix run [--model NAME] [--max-steps N] [-c|--continue] [--resume PATH] [--copy] [--output-format FORMAT] <task>
  reasonix run --events-jsonl [--model NAME] <task>      emit redacted structured events as JSONL
  reasonix review [--base BRANCH] [--commit SHA] [--model NAME]  AI-powered code review on local diffs
  reasonix web [--model NAME] [--addr HOST:PORT] [--no-open]  start the local Web UI and open it in the default browser
  reasonix serve [--model NAME] [--addr HOST:PORT] [--auth none|token|password] [--token STR] [--password STR] [--hash-password]  serve over HTTP+SSE (with optional auth)
  reasonix acp [--model NAME]                           serve Agent Client Protocol over stdio (also: reasonix --acp)
  reasonix setup [path]                                 interactive config wizard; writes reasonix.toml (+ .env)
  reasonix config reasoning-language [auto|zh|en]        configure visible reasoning language
  reasonix config compact-ratio [--local] [30..85]       configure automatic compaction threshold
  reasonix config telemetry [auto|on|off]                configure content-free CLI usage metrics
  reasonix report [list|show|send|delete] [ID]           review and explicitly send local CLI crash reports
  reasonix mcp <add|remove|list|import>                 manage MCP servers in reasonix.toml
  reasonix subagent <list|create|edit|delete|try|run>   manage and run isolated subagent profiles
  reasonix init                                         show how to generate project memory (AGENTS.md)
  reasonix doctor [--json]                              print redacted local diagnostics
  reasonix doctor session <branch-id> [--zip] [--out PATH]  export a session conflict diagnostic zip
  reasonix session list --json [--dir PATH]             list redacted sessions for machine clients
  reasonix session show|status <machine-session-id> --json [--dir PATH]  query one redacted session
  reasonix session recovery [<machine-session-id>] --json [--dir PATH]  query redacted recovery state
  reasonix hook list|status --json [--dir PATH]         inspect redacted hook state
  reasonix trust [--dir PATH] [--yes|--revoke]          review and approve programs this project runs
  reasonix task list|show|status|events|stop|cancel|monitor|tmux --json [--dir PATH]
                                                         inspect or control redacted tasks
  reasonix bot start|doctor|weixin-login                multi-channel IM bot gateway
  reasonix upgrade [--check] [--force]                   update to the latest official release (also: reasonix update)
  reasonix completion bash|zsh|fish                     print a shell completion script to stdout
  reasonix version [--verbose|--json]                   print version (single line) or build metadata
  reasonix --version | -v                               single-line version (script-safe)
  reasonix help

Examples:
  reasonix
  reasonix --continue
  reasonix --resume provider-config
  reasonix web
  reasonix run "implement the TODOs in main.go"
  reasonix run --model mimo-pro "add unit tests for this function"
  reasonix -p "summarize this repository" --output-format json
  reasonix subagent run review "review the current changes"
  echo "explain this code" | reasonix run

Configuration:
  Resolution: flag > ./reasonix.toml > <Reasonix home>/config.toml > built-in defaults
  Secrets come from the environment via api_key_env (e.g. DEEPSEEK_API_KEY).
  Run 'reasonix setup' to scaffold a config; see docs/SPEC.md. tool to run TWO subagents in parallel, one per crate, each in its own
git worktree so their workspaces never collide:
  git worktree add .work/protocol -b recon/protocol
  git worktree add .work/lich -b recon/lich
Subagent A works in .work/protocol on crate hydra_protocol. Subagent B works in .work/lich on crate hydra_lich.
Wait for both, then merge both branches into the main branch of ~/Code/hydra-recon, resolve conflicts, run
cargo check --workspace and cargo test -p hydra_protocol -p hydra_lich, and report.

## Per-crate job (give each subagent this text)
1. For each stub fn in your crate, disassemble it (otool -tV / objdump -d --disassemble-symbols, plus strings and
   otool -s __TEXT __cstring for literals) and recover: parameter types, return type, and the behaviour.
2. Replace each todo!() with a working Rust implementation. Define the struct/enum types with the fields you can
   establish from the code. Where a detail is unknown, keep a clearly marked "// UNKNOWN:" comment and the
   safest behaviour, rather than inventing.
3. hydra_protocol is a streaming parser for the GemStone IV XML-ish wire format (parser, text/entities, tags, runs, numbers,
   frame::payload, write). Add unit tests with literal sample lines (for example <pushBold/>, <a exist=..>, &lt; entities,
   amounts like "1,234 silvers", durations). Tests must assert real values.
4. hydra_lich is the Lich relay/child-process integration (relay, child, find_ruby, new_token). Document the IPC and
   password-prompt handling in crates/hydra_lich/NOTES.md.
5. Gate: cargo check and cargo test pass in the worktree before you finish. Leave all other crates untouched.
   Acceptance checks are questions, not targets: report conflicts honestly instead of working around them.
6. Commit in the worktree branch with clear messages. Your deliverables are only files under your crate directory
   (plus NOTES.md). Work only inside your worktree: no ssh, no network, no other hosts.
7. Finish with a short report: functions fully recovered, partly recovered, and unknown.
