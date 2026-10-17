/**
 * ⌘K omnibox.
 *
 * A Radix `Dialog.Portal`, which is the whole reason this app is React: the identical component renders
 * *nothing* through `preact/compat`, throwing `InvalidCharacterError`. Focus trapping, dismissal and
 * ARIA wiring come from the primitive rather than from hand-rolled code.
 *
 * Opening it must not lose the Focus — the subject is in the URL, and the palette only ever writes to it.
 */
import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useMemo, useState } from "react";
import type { FunctionEntry } from "./data";

export function Palette({
  open,
  onOpenChange,
  functions,
  onPick,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  functions: FunctionEntry[];
  onPick: (entry: FunctionEntry) => void;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);

  const matches = useMemo(() => {
    const q = query.trim().toLowerCase();
    const pool = q
      ? functions.filter(
          (f) => f.name.toLowerCase().includes(q) || (f.file_path ?? "").toLowerCase().includes(q),
        )
      : functions;
    return pool.slice(0, 40);
  }, [functions, query]);

  useEffect(() => setActive(0), [query]);

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/60" />
        <Dialog.Content
          aria-describedby={undefined}
          className="fixed left-1/2 top-24 z-50 w-[min(680px,92vw)] -translate-x-1/2 rounded-lg border border-white/12 bg-[#12151b] shadow-2xl"
        >
          <Dialog.Title className="sr-only">Search symbols</Dialog.Title>
          <div className="flex items-center gap-2 border-b border-white/8 px-3 py-2">
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") { e.preventDefault(); setActive((i) => Math.min(i + 1, matches.length - 1)); }
                if (e.key === "ArrowUp") { e.preventDefault(); setActive((i) => Math.max(i - 1, 0)); }
                if (e.key === "Enter" && matches[active]) { e.preventDefault(); onPick(matches[active]); }
              }}
              placeholder="Search symbols, or paste a stable key…"
              className="w-full bg-transparent text-[13px] text-zinc-100 outline-none placeholder:text-zinc-600"
            />
            <span className="rounded border border-white/10 px-1.5 py-0.5 text-[10px] text-zinc-500">esc</span>
          </div>
          <div className="max-h-[52vh] overflow-y-auto p-1">
            {matches.length === 0 && (
              <p className="px-2 py-3 text-[12px] text-zinc-500">
                No function matches. The bundle carries {functions.length} function{functions.length === 1 ? "" : "s"}.
              </p>
            )}
            {matches.map((f, i) => (
              <button
                key={f.function_id}
                type="button"
                onMouseEnter={() => setActive(i)}
                onClick={() => onPick(f)}
                className={`flex w-full items-center gap-3 rounded px-2 py-1.5 text-left ${
                  i === active ? "bg-white/8" : "hover:bg-white/[0.04]"
                }`}
              >
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[12px] text-zinc-100">{f.name}</span>
                  <span className="block truncate font-mono text-[10px] text-zinc-500">{f.file_path ?? "—"}</span>
                </span>
                <span className="shrink-0 font-mono text-[10px] text-zinc-600">
                  {f.pdg_nodes ?? "—"} pdg · {f.block_count ?? "—"} blk
                </span>
              </button>
            ))}
          </div>
          <div className="flex items-center justify-between border-t border-white/8 px-3 py-1.5 text-[10px] text-zinc-600">
            <span>↑↓ move · ⏎ focus</span>
            <span>{matches.length} of {functions.length} candidates</span>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
