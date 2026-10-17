/** Small presentational primitives shared by the shell and rooms. */
import type { ReactNode } from "react";

export function Panel({
  title,
  subtitle,
  right,
  children,
}: {
  title?: string;
  subtitle?: string;
  right?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="rounded-lg border border-white/8 bg-white/[0.02]">
      {(title || right) && (
        <header className="flex items-baseline justify-between gap-3 border-b border-white/8 px-3 py-2">
          <div className="min-w-0">
            {title && <h2 className="truncate text-[12px] font-medium tracking-wide text-zinc-200">{title}</h2>}
            {subtitle && <p className="mt-0.5 text-[11px] text-zinc-500">{subtitle}</p>}
          </div>
          {right}
        </header>
      )}
      <div className="p-3">{children}</div>
    </section>
  );
}

export function Chip({ children, tone = "zinc" }: { children: ReactNode; tone?: "zinc" | "amber" | "green" | "blue" }) {
  const tones = {
    zinc: "bg-white/6 text-zinc-400 border-white/10",
    amber: "bg-amber-500/10 text-amber-300 border-amber-500/25",
    green: "bg-emerald-500/10 text-emerald-300 border-emerald-500/25",
    blue: "bg-sky-500/10 text-sky-300 border-sky-500/25",
  };
  return (
    <span className={`inline-flex items-center rounded border px-1.5 py-0.5 text-[10px] leading-none ${tones[tone]}`}>
      {children}
    </span>
  );
}

export function Stat({ label, value, hint }: { label: string; value: ReactNode; hint?: string }) {
  return (
    <div className="min-w-0">
      <div className="text-[10px] uppercase tracking-wider text-zinc-500">{label}</div>
      <div className="mt-0.5 truncate text-[15px] font-medium text-zinc-100">{value}</div>
      {hint && <div className="truncate text-[10px] text-zinc-600">{hint}</div>}
    </div>
  );
}

/**
 * An absence with a reason.
 *
 * The shell's honesty rule, made structural: a surface that cannot produce something says so and says
 * why, rather than rendering an empty container a reader will interpret as "nothing to see". This is
 * the difference between "no violations" and "policy never ran".
 */
export function Unavailable({
  what,
  why,
  // "not in this bundle" is a claim about *availability*, and it is the wrong claim most of the time:
  // an unbuilt lens is absent because no code reads data that is present, and a blocked lens is absent
  // because two artifacts share no key. A hard-coded badge contradicted the explanation under it.
  tag = "not in this bundle",
}: {
  what: string;
  why: string;
  tag?: string;
}) {
  return (
    <div className="rounded-lg border border-amber-500/25 bg-amber-500/[0.04] p-3">
      <div className="flex items-center gap-2">
        <Chip tone="amber">{tag}</Chip>
        <span className="text-[12px] text-zinc-200">{what}</span>
      </div>
      <p className="mt-1.5 text-[11px] leading-relaxed text-zinc-400">{why}</p>
    </div>
  );
}

/** Monospace receipt: the exact command behind a number. */
export function Command({ children }: { children: ReactNode }) {
  return (
    <code className="mt-2 block overflow-x-auto rounded border border-white/8 bg-black/40 px-2 py-1 font-mono text-[11px] text-emerald-200/90">
      {children}
    </code>
  );
}
