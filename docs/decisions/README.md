# Architecture decision records

One file per decision, numbered and never rewritten. To change your mind,
add a new record that says `Supersedes: NNNN` and set the old one's
status to `Superseded by NNNN`.

| # | Decision | Status |
|---|---|---|
| [0001](0001-wine-not-recompilation.md) | Run the original under Wine instead of recompiling or rewriting it | Accepted |
| [0002](0002-xpra-html5-streaming.md) | Stream with Xpra's HTML5 client from the Ubuntu archive | Accepted |
| [0003](0003-bring-your-own-discs.md) | Ship no content; the user supplies the discs | Accepted |
| [0004](0004-reproduce-installer-headless.md) | Reproduce the installer with scripts instead of running its GUI | Accepted |
| [0005](0005-no-activation-bypass.md) | Do not bypass or generate activation codes | Accepted |
| 0006 | Experiment: Wine 11 new-WoW64 (branch `experiment/wine11-wow64`) | Rejected for Apple Silicon |
| [0007](0007-boxedwine-web.md) | Investigate Boxedwine WebAssembly as an alternative runtime | Accepted for experimentation (branch) |

## Template

```markdown
# NNNN — Title in the imperative

- Status: Proposed | Accepted | Superseded by NNNN
- Date: YYYY-MM-DD

## Problem
What hurts, for whom. One paragraph.

## Decision
What we do.

## Alternatives considered
- Option — why not.

## Consequences
What becomes easier, what becomes harder, what to watch for.

## Evidence
How we know (test, measurement, diff, link).
```
