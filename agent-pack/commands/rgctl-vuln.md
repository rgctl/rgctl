---
name: /rgctl:vuln
id: rgctl-vuln
category: Workflow
description: >-
  Triage an OSV/CVE advisory, scan a repo, or generate a threat model &
  STRIDE attack-surface review with rgctl vuln (fetch / scan / triage /
  deps check / analyze / threat-model): package match, parked call-site
  reachability, OpenVEX status, Mermaid DFD, and evidence-backed STRIDE
  candidates. Session-only — no forge posting.
---

Run the **rgctl-vuln** skill protocol for this repository.

**Canonical skill:** `rgctl-vuln` (installed next to meta skill `rgctl`).

**Input:** Optional args after the command — path to an OSV JSON file or directory, `threat-model` / `surface` request, optional `--include-jars <dir>`, or free-text describing the CVE/advisory or threat-modeling scope.

Follow `skills/rgctl-vuln/SKILL.md` (or the host’s installed copy under the agent skills root):

1. Ensure `.rgctl/graph.snapshot.bin` exists (`rgctl discover .` if missing; lazy per-sink CFG/PDG runs automatically)
2. **Mode A (Whole-repo scan):** `rgctl -f json vuln scan` (or `--offline` / `--from-report`)
3. **Mode B (Single advisory):** `rgctl -f json vuln triage --osv …` → `deps check --osv …` → `vuln analyze --osv …`
4. **Mode C (Threat model & STRIDE):** `rgctl -f json vuln threat-model --offline` (optional `-f mermaid` DFD), keeping observed `graph_fact`/`cpg_taint`/`osv_reachability` strictly separated from `graph_rule`/`agent_inference` (`needs_review`)
5. Optional `slice` on hit call sites; emit concise CVE Reachability or Threat Model report

Do **not** post to GitHub/GitLab. Do **not** start `rgctl serve` / `--watch`.
