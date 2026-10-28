# 0005 — Do not bypass or generate activation codes

- Status: Accepted
- Date: 2026-10-28

## Problem
iLumina needs online activation. The service is gone, so unactivated
copies are limited to a 15-day trial. That is a real obstacle to
preservation.

## Decision
The project does not patch, crack, reverse-engineer or generate activation
codes. It does not reset the trial either (for example by recreating the
prefix on every session). It only **types a code the owner already has**
(`ILUMINA_ACTIVATION_CODE`) into the normal dialog.

## Alternatives considered
- **Reverse-engineer `ActCode_Generate` / `BoxCode_*` in the Lingo**:
  circumvention, and it would make the repository unpublishable.
- **Ephemeral prefixes that restart the trial**: the same effect by other
  means.

## Consequences
- ❌ Most users will run in trial mode or with a code tied to an old PC.
- The legitimate route is the rights holders: a preservation licence, a
  universal code or permission. Track that outside the code.
- Agents: requests to "make activation go away" are out of scope. Point to
  this record.
