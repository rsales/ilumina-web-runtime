# Documentation

This folder is the **canonical, long-lived** documentation of the project.
If something must still be true and findable a year from now, it goes here,
not in a chat, an issue or a commit message.

It is written for two readers: people who contribute, and AI coding agents
(Claude Code, Codex, Cursor…). Both have the same problem. They don't know
the history, and they can't see what happened in the previous session.

## Reading order

| # | File | Read it when |
|---|---|---|
| 1 | [use-cases.md](use-cases.md) | Always first. What problems this project solves, and which ones it deliberately doesn't |
| 2 | [architecture.md](architecture.md) | Before touching `Dockerfile` or `runtime/` |
| 3 | [compatibility.md](compatibility.md) | Before you assume a feature works, or when something breaks |
| 4 | [decisions/](decisions/README.md) | Before you propose "why not just use X?" — it may already have been decided |
| 5 | [runbooks/](runbooks/) | When you need to *do* something: run, debug, re-measure the installer |
| 5b | [port-test-matrix.md](port-test-matrix.md) | When working on the Rust port (ADR 0008) |
| 6 | [history.md](history.md) | To know what the original product was, its 2005 requirements and the publisher's own media FAQ |
| 7 | [memory/](memory/README.md) | At the start and end of every work session: handoff and learnings |

## What lives where

| Kind of information | Where | Lifetime |
|---|---|---|
| What the project is for and how to start | `/README.md` | stable, short |
| Rules for agents and contributors | `/AGENTS.md` | stable |
| Use cases, architecture, compatibility | `docs/*.md` | canonical, kept up to date |
| Why we chose A over B | `docs/decisions/NNNN-*.md` | append-only (supersede, don't rewrite) |
| How to perform a task | `docs/runbooks/*.md` | updated when the procedure changes |
| Current state, next steps, open threads | `docs/memory/handoff.md` | overwritten every session |
| Facts learned the hard way | `docs/memory/learnings.md` | append, then promote to canonical docs |
| What changed per version | `/CHANGELOG.md` | append-only |

## Writing rules

- **Use case before technology.** Start every document with the problem it
  solves. "Lets a QuickTime installer finish unattended" is better than
  "xdotool wrapper".
- **Evidence over belief.** Mark every claim as verified (✅, with how it
  was verified), observed (⚠️) or untested (⏳). Never write "works" for
  something nobody ran.
- **No original content.** No screenshots, extracted text, media or code
  from iLumina in this repository. Hashes and file names are fine.
- Keep it short. Link instead of repeating.
