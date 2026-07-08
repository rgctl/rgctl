import type { ComponentChildren } from "preact";
import { useEffect, useState } from "preact/hooks";
import { TAB_DOCS, type TabId } from "./tabDocs";

export interface TabDocPanelProps {
  tabId: TabId;
  defaultOpen?: boolean;
}

export function TabDocPanel({ tabId, defaultOpen = false }: TabDocPanelProps) {
  const [open, setOpen] = useState(defaultOpen);
  const doc = TAB_DOCS[tabId];

  useEffect(() => {
    setOpen(defaultOpen);
  }, [tabId, defaultOpen]);

  return (
    <div class="rb-tab-doc-panel border-bottom bg-light flex-shrink-0">
      <button
        type="button"
        class="rb-tab-doc-toggle w-100 d-flex align-items-center gap-2 px-3 py-2 btn btn-link text-decoration-none text-start"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <span class="rb-tab-doc-chevron" aria-hidden="true">
          {open ? "▾" : "▸"}
        </span>
        <span class="fw-semibold small text-body">{doc.title}</span>
        <span class="text-muted small ms-1">— how to use this view</span>
      </button>

      {open && (
        <div class="rb-tab-doc-body px-3 pb-3 small">
          <p class="mb-2">
            <span class="fw-semibold">Goal: </span>
            {doc.goal}
          </p>
          <p class="text-muted mb-3">{doc.description}</p>

          <div class="row g-3">
            <div class="col-md-6">
              <div class="fw-semibold mb-1">Key benefits</div>
              <ul class="mb-0 ps-3">
                {doc.benefits.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            </div>
            <div class="col-md-6">
              <div class="fw-semibold mb-1">In this dashboard</div>
              <ul class="mb-0 ps-3">
                {doc.usage.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export function TabPanelStack({
  tabId,
  children,
}: {
  tabId: TabId;
  children: ComponentChildren;
}) {
  return (
    <div class="rb-tab-panel-stack d-flex flex-column h-100 min-h-0" key={tabId}>
      <TabDocPanel tabId={tabId} />
      <div class="rb-tab-panel-main flex-grow-1 min-h-0 d-flex flex-column">{children}</div>
    </div>
  );
}
