/** Wait until an element has non-zero layout dimensions (flex panels settle async). */
export async function waitForContainerSize(
  el: HTMLElement,
  maxFrames = 40,
): Promise<boolean> {
  for (let i = 0; i < maxFrames; i++) {
    if (el.offsetHeight > 0 && el.offsetWidth > 0) return true;
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  }
  return el.offsetHeight > 0 && el.offsetWidth > 0;
}

/**
 * Mount Sigma only after the container has measurable size.
 * Uses ResizeObserver so late layout (e.g. first paint) still triggers init.
 */
export function mountSigmaWhenReady(
  container: HTMLElement,
  mount: () => (() => void) | void,
): () => void {
  let disposed = false;
  let innerCleanup: (() => void) | undefined;
  let ro: ResizeObserver | null = null;

  const tryMount = () => {
    if (disposed || innerCleanup) return;
    if (container.offsetHeight === 0 || container.offsetWidth === 0) return;
    innerCleanup = mount() ?? undefined;
    if (innerCleanup && ro) {
      ro.disconnect();
      ro = null;
    }
  };

  void (async () => {
    await waitForContainerSize(container);
    tryMount();
  })();

  if (!innerCleanup) {
    ro = new ResizeObserver(() => tryMount());
    ro.observe(container);
  }

  return () => {
    disposed = true;
    ro?.disconnect();
    innerCleanup?.();
  };
}
