/**
 * The CFG sidecar, loaded once for every surface that draws it.
 *
 * The Spine lens and the CFG·PDG lens render the same `cfg/<function_id>.json`, so they read it through
 * one loader rather than each owning an effect. Two effects would be two chances to disagree about what
 * an absent sidecar means.
 *
 * The state is four-valued on purpose. With `CfgDetail | null` as the only state, "still fetching" and
 * "this bundle has no CFG for this subject" are the same value — and the Spine rendered the second one as
 * `loading…` forever, which is the exact failure the shell's honesty rule exists to prevent: a reader
 * cannot tell "not built yet" from "built and found nothing".
 */
import { useEffect, useState } from "react";
import { loadCfgDetail, type CfgDetail } from "./data";

export type CfgState =
  | { kind: "loading" }
  | { kind: "ready"; cfg: CfgDetail }
  /** A typed absence: the bundle is complete, and this subject is not in it. */
  | { kind: "absent"; reason: string }
  /** The read itself failed — a different claim from "absent", so it is a different state. */
  | { kind: "failed"; reason: string };

export function useCfgDetail(functionId: string): CfgState {
  const [state, setState] = useState<CfgState>({ kind: "loading" });

  useEffect(() => {
    let cancelled = false;
    setState({ kind: "loading" });
    loadCfgDetail(functionId)
      .then((detail) => {
        if (cancelled) return;
        setState(
          detail
            ? { kind: "ready", cfg: detail }
            : {
                kind: "absent",
                reason:
                  "The CFG export writes one file per function the analysis phase reached, and this bundle has no `cfg/<function_id>.json` for this subject. There is no control-flow graph to draw — as opposed to one that came back empty.",
              },
        );
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        // A 404 is handled above; anything reaching here is a real read failure and says so instead of
        // settling into a permanent "loading" that looks like a slow disk.
        setState({ kind: "failed", reason: String(err) });
      });
    return () => {
      cancelled = true;
    };
  }, [functionId]);

  return state;
}
