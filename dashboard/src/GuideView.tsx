import { CLI_WORKFLOWS, GUIDE_PREREQUISITES } from "./guideCliWorkflows";

function CommandBlock({ comment, commands }: { comment?: string; commands: string[] }) {
  return (
    <div class="mb-3">
      {comment && <div class="text-muted mb-1">{comment}</div>}
      <pre class="bg-light border rounded p-2 small mb-0 guide-cli-pre">{commands.join("\n")}</pre>
    </div>
  );
}

export function GuideView() {
  return (
    <div class="guide-view p-4 overflow-auto">
      <p class="text-muted mb-4">
        Each dashboard tab has a CLI workflow below. Run commands from your repository root after{" "}
        <code>discover</code>. Replace example symbols (<code>ShoppingCartService</code>, paths) with
        names from your project. Use <code>-r "$REPO"</code> when not in the repo directory.
      </p>

      <section class="mb-4">
        <h3 class="h6 fw-semibold mb-2">Prerequisites</h3>
        <pre class="bg-light border rounded p-3 small mb-0 guide-cli-pre">{GUIDE_PREREQUISITES}</pre>
      </section>

      {CLI_WORKFLOWS.map((section) => (
        <section key={section.tabId} class="card shadow-sm mb-4" id={`cli-${section.tabId}`}>
          <div class="card-header py-2">
            <h3 class="h6 mb-0 fw-semibold">{section.tabLabel}</h3>
          </div>
          <div class="card-body small">
            <p class="mb-2">{section.summary}</p>
            {section.prerequisite && (
              <p class="mb-3">
                <span class="fw-semibold">Requires: </span>
                <code>{section.prerequisite}</code>
              </p>
            )}
            {section.blocks.map((block, i) => (
              <CommandBlock key={i} comment={block.comment} commands={block.commands} />
            ))}
            {section.notes && section.notes.length > 0 && (
              <ul class="text-muted mb-0 ps-3">
                {section.notes.map((note) => (
                  <li key={note}>{note}</li>
                ))}
              </ul>
            )}
          </div>
        </section>
      ))}

      <section class="card shadow-sm mb-2 border-secondary">
        <div class="card-body small text-muted">
          <span class="fw-semibold text-body">Further reading: </span>
          <code>docs/user-guide.md</code>, <code>docs/cli-getting-started.md</code>,{" "}
          <code>docs/json-api.md</code>
        </div>
      </section>
    </div>
  );
}
