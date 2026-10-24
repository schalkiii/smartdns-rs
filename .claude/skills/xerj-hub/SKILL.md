---
name: xerj-hub
description: Answer a question about public source code, a standard, a regulation or an advisory feed from a pre-indexed hub corpus with one command, xerj "your question". Use it for exact public material you cannot quote from memory. Not for this project's own files.
---
<!-- xerj-init-version: 5 sha256:131aa9224f56d182 -->

# XERJ hub: ask, do not clone

Pick the source before you search:

- **HUB**: public material you cannot quote exactly. A library's source at a pinned commit, a
  standard, a regulation, an advisory feed.
- **LOCAL**: files only this machine has. This project, private documents, exports.
  `xerj autoindex <folder> --prefix <name>` once, then pass that `--prefix <name>` to
  `xerj search` and `xerj def`. Without `--prefix` they search the default `ax` prefix.
- **NEITHER**: a fact you can already state exactly. Retrieval on memorised content only adds cost.

## Ask the hub

```sh
xerj "<question>"
```

One line. No corpus name, no clone, no index. XERJ picks the corpus (your project's dependencies
first), fetches its signed, pre-indexed pack on first use, and answers. Retrieval is lexical, not
semantic: use the words and identifiers the source uses. The bare form needs a space in the
question; for a single word write `xerj ask "<word>"`.

A good answer is three lines, then any `warning:` lines:

```text
<corpus>  pin <date> (<N> days old)  <licence>  <file>:<line>
<the passage, or the declaration>
answered from: local pack
```

`pin` is the source revision the corpus was built from: check its age when you ask about the current
version. `answered from:` can also say `hosted hub`. Tell the user which corpus, pin date and licence
your answer rests on. `warning: approach-only licence` means read the source, do not copy it.

## Exact questions

```sh
xerj ask --def <SYMBOL>
```

Use `--def` for a value, a signature or a default. It prints the definition (`file:line`, the
declaration, and a `value:` line for a constant or macro) and no passages. Add `--corpus <slug>` when
more than one corpus defines the name.

## When it does not answer

- Exit 4: more than one corpus fits. The candidates print, one per line. Run again with
  `--corpus <slug>`.
- Exit 1, `no hub corpus matches`: the hub does not hold it. Say so. Do not answer from memory.
- Exit 3: a downloaded pack failed its checksum or signature check and was discarded. Do not use it.
  Tell the user.
- Exit 5: this build has no hub public key, so no pack can be verified and nothing was downloaded.
  Tell the user; a retry will not help.
- `run: xerj hub pull <slug> (<MB> MB)`: the pack is above the automatic download limit. Run that
  line, then ask again. Success: `pulled <slug> <pin8> <bytes> → <n> indices, <defs> definitions`.

## The node, and where the question goes

If a node answers on `--url`, `$XERJ_URL` or `localhost:9200` (first one set), XERJ uses it as it
is. If none does, XERJ starts its own on a free loopback port and records its PID.
Stop only the node you started: `xerj hub stop` for XERJ's own (success: `stopped the hub node
(pid <N>)` or `no hub node was started by xerj; nothing to stop`); for a node you started by hand,
kill only the PID you recorded when you started it. Never `pkill xerj` or `killall xerj`: other XERJ nodes may be running
on this machine.

By default, inside a git repository the question is answered on this machine. Outside one it may be
sent to the hosted hub. `xerj hub config hosted off` forbids that.

## MCP

`xerj mcp` serves three hub tools. They run the same flow as `xerj ask`:
- `xerj_hub_ask` `{question, corpus?, mode?, max_tokens?}`: the shell line above, as one call.
- `xerj_hub_def` `{slug, symbol, k?}`: `xerj ask --corpus <slug> --def <SYMBOL>`.
- `xerj_hub_find` `{question, k?}`: which corpora could answer, best first. No passages.

A hub tool answers only from packs already on this machine; it never downloads one. When a corpus
is not here, the answer is the `xerj hub pull <slug>` line: run it in a shell, then ask again.
Nor does a hub tool fetch the catalogue: it reads the cached copy. If it says no catalogue is
cached, run `xerj hub sync` in a shell once, then call the tool again.

Register `xerj mcp` once, then restart the session. Success: `xerj_search` is in the tool list.
Use the absolute path to the binary: an editor started from a desktop icon does not inherit your
shell PATH. `command -v xerj` prints it (PowerShell: `(Get-Command xerj).Source`).

- Claude Code: `claude mcp add --scope user xerj -- "$(command -v xerj)" mcp`
  (PowerShell or cmd: put the full path of `xerj.exe` in place of `"$(command -v xerj)"`)
- `.mcp.json` (Claude Code, project scope; `xerj init` writes it), Cursor, Windsurf, Claude Desktop,
  Cline: root key `mcpServers`.
  `{"mcpServers": {"xerj": {"command": "/abs/path/to/xerj", "args": ["mcp"]}}}`
- VS Code (`.vscode/mcp.json`): root key `servers`, not `mcpServers`. A block under the wrong key is
  silently ignored.
  `{"servers": {"xerj": {"type": "stdio", "command": "/abs/path/to/xerj", "args": ["mcp"]}}}`
- Codex (`.codex/config.toml`, trusted projects only; `xerj init` writes it when `.codex/` exists):
  `[mcp_servers.xerj]` with `command`, `args = ["mcp"]`, and `[mcp_servers.xerj.env]` `XERJ_URL`.
