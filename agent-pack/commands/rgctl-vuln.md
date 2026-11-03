---
name: /rgctl:vuln
id: rgctl-vuln
category: Workflow
description: >-
  Triage an OSV/CVE advisory with rgctl vuln (triage / deps check / analyze):
  package match, parked call-site reachability, OpenVEX status, and a short
  CVE Reachability report. Session-only — no forge posting.
---

Run the **rgctl-vuln** skill protocol for this repository.

**Canonical skill:** `rgctl-vuln` (installed next to meta skill `rgctl`).

**Input:** Optional args after the command — path to an OSV JSON file or directory, optional `--include-jars <dir>`, or free-text describing the CVE/advisory to triage.

Follow `skills/rgctl-vuln/SKILL.md` (or the host’s installed copy under the agent skills root):

1. Verify CFG readiness (`cpg status`; `discover . --with-cfg` if missing)
2. `rgctl -f json vuln triage --osv …`
3. `rgctl -f json deps check --osv …` (add `--include-jars` when shaded JARs exist)
4. `rgctl -f json vuln analyze --osv …` → read `.sink_taint.call_sites[]` / `.sink_taint.paths[]`
5. Optional `slice` on hit call sites; emit CVE Reachability & OpenVEX report

Do **not** post to GitHub/GitLab. Do **not** start `rgctl serve` / `--watch`.
