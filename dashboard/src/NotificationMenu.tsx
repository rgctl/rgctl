import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import type { DashboardManifest, EngineReady } from "./types";

export interface NotificationItem {
  id: string;
  title: string;
  detail?: string;
  tone: "ok" | "info" | "warn" | "error";
}

export interface NotificationMenuProps {
  manifest: DashboardManifest | null;
  engine: EngineReady | null;
  wasmReady: boolean;
  manifestError?: string | null;
  workerError?: string | null;
}

function toneClass(tone: NotificationItem["tone"]): string {
  switch (tone) {
    case "ok":
      return "text-success";
    case "warn":
      return "text-warning";
    case "error":
      return "text-danger";
    default:
      return "text-muted";
  }
}

function buildNotifications({
  manifest,
  engine,
  wasmReady,
  manifestError,
  workerError,
}: NotificationMenuProps): NotificationItem[] {
  const items: NotificationItem[] = [];

  if (manifestError) {
    items.push({
      id: "manifest-error",
      title: "Manifest failed to load",
      detail: manifestError,
      tone: "error",
    });
  } else if (!manifest) {
    items.push({
      id: "manifest-loading",
      title: "Loading dashboard manifest…",
      tone: "info",
    });
  } else {
    items.push({
      id: "manifest-ready",
      title: "Dashboard manifest loaded",
      detail: `v${manifest.schema_version} · ${manifest.graph.payload_format}`,
      tone: "ok",
    });
  }

  if (workerError) {
    items.push({
      id: "worker-error",
      title: "Analysis engine error",
      detail: workerError,
      tone: "error",
    });
  }

  if (!engine) {
    items.push({
      id: "engine-loading",
      title: "Starting analysis engine…",
      tone: "info",
    });
  } else if (engine.wasm && wasmReady) {
    items.push({
      id: "wasm-ready",
      title: "WASM engine ready",
      detail: `${engine.nodeCount.toLocaleString()} nodes · ${engine.edgeCount.toLocaleString()} edges`,
      tone: "ok",
    });
  } else if (engine.wasm) {
    items.push({
      id: "wasm-loading",
      title: "Loading WASM graph engine…",
      detail: "Serve the dashboard over HTTP if this persists.",
      tone: "warn",
    });
  } else {
    items.push({
      id: "wasm-fallback",
      title: "WASM unavailable",
      detail: "Using JavaScript fallback. Interactive analysis may be limited.",
      tone: "warn",
    });
  }

  if (manifest?.view) {
    items.push({
      id: "metagraph",
      title: "Graph overview ready",
      detail: `${manifest.view.metanode_count} metanodes · ${manifest.view.metaedge_count} metaedges`,
      tone: "ok",
    });
  }

  const pendingPhases = manifest
    ? Object.entries(manifest.phases).filter(([, status]) => status !== "complete")
    : [];
  if (pendingPhases.length > 0) {
    items.push({
      id: "phases-pending",
      title: "Some analysis exports are pending",
      detail: pendingPhases.map(([phase]) => phase).join(", "),
      tone: "warn",
    });
  } else if (manifest) {
    items.push({
      id: "phases-complete",
      title: "Analysis exports complete",
      tone: "ok",
    });
  }

  return items;
}

export function NotificationMenu({
  manifest,
  engine,
  wasmReady,
  manifestError,
  workerError,
}: NotificationMenuProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const notifications = useMemo(
    () =>
      buildNotifications({
        manifest,
        engine,
        wasmReady,
        manifestError,
        workerError,
      }),
    [manifest, engine, wasmReady, manifestError, workerError],
  );

  const issueCount = notifications.filter((n) => n.tone === "error" || n.tone === "warn").length;

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div class="rb-notifications" ref={rootRef}>
      <button
        type="button"
        class="btn btn-outline-secondary btn-sm rb-notifications-toggle position-relative"
        aria-expanded={open}
        aria-haspopup="true"
        aria-label="System notifications"
        onClick={() => setOpen((value) => !value)}
      >
        <span class="rb-notifications-icon" aria-hidden="true">
          🔔
        </span>
        {issueCount > 0 && (
          <span class="rb-notifications-badge badge rounded-pill bg-danger">
            {issueCount}
          </span>
        )}
      </button>

      {open && (
        <div class="rb-notifications-panel card shadow border" role="menu">
          <div class="card-header py-2 px-3 small fw-semibold d-flex align-items-center justify-content-between">
            <span>Notifications</span>
            <span class="text-muted fw-normal">{notifications.length}</span>
          </div>
          <ul class="list-group list-group-flush rb-notifications-list">
            {notifications.map((item) => (
              <li class="list-group-item py-2 px-3 small" key={item.id} role="none">
                <div class={`fw-semibold ${toneClass(item.tone)}`}>{item.title}</div>
                {item.detail && <div class="text-muted mt-1">{item.detail}</div>}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
