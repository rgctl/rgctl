---
name: /rgctl:review
id: rgctl-review
category: Workflow
description: >-
  Review a PR or branch with rgctl review paths/check; write REVIEW_REPORT.md
  and a four-section session verdict (no forge posting).
---

Run the **rgctl-review** skill protocol for this repository.

**Canonical skill:** `rgctl-review` (installed next to meta skill `rgctl`).

**Input:** Optional args after the command — base/head refs (default `origin/main`…`HEAD`), or free-text describing what to review.

Follow `skills/rgctl-review/SKILL.md` (or the host’s installed copy under the agent skills root):

1. Resolve/create base artifact (`.rgctl-base` / `$RGCTL_BASE_ARTIFACT`)
2. `rgctl -f json review paths …` → `.rgctl/review-paths.json`
3. Optional blast-radius on top symbols; `review check` if policy exists
4. Soft architecture-md pass (≤5 files; never alone → RED)
5. Write **`REVIEW_REPORT.md`**, then emit Verdict + four-section summary + Guidelines in session

Do **not** post to GitHub/GitLab. Do **not** start `rgctl serve` / `--watch`.
