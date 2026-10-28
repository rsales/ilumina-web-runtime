# Project memory

Short-lived context that keeps work moving between sessions and between
agents (Claude Code, Codex, Cursor…). It plays the same role as a shared
Markdown wiki such as Akita's *ai-memory*, kept in the repo so every
agent and contributor sees the same thing.

| File | Purpose | Rule |
|---|---|---|
| [handoff.md](handoff.md) | Where the work stopped and what comes next | **Overwrite** at the end of every session |
| [learnings.md](learnings.md) | Facts discovered the hard way (gotchas, measurements) | **Append**, dated. Once a fact is stable, promote it into `docs/*.md` or an ADR and mark it `→ promoted` |

Chat transcripts and model context are disposable. If it matters, write it
here or in the canonical docs before the session ends.

If you use an external memory tool (ai-memory, project memory in Claude
Code, etc.), keep it in sync with these files. The repository copy wins.
