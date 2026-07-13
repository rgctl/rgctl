import type Sigma from "sigma";
import { debounce } from "./debounce";

const MIN_SIGMA_SIZE = 32;
/** WebGL max texture dimension on most GPUs; avoid texImage2D out-of-range errors. */
const MAX_CANVAS_DIM = 4096;
const RESIZE_DEBOUNCE_MS = 120;

/** Wait until an element has usable layout dimensions (flex panels settle async). */
export async function waitForContainerSize(
  el: HTMLElement,
  maxFrames = 120,
  minSize = MIN_SIGMA_SIZE,
): Promise<boolean> {
  for (let i = 0; i < maxFrames; i++) {
    if (isValidCanvasSize(el)) return true;
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  }
  return isValidCanvasSize(el);
}

export function clampCanvasDim(n: number): number {
  return Math.max(MIN_SIGMA_SIZE, Math.min(MAX_CANVAS_DIM, Math.floor(n)));
}

export function isValidCanvasSize(el: HTMLElement): boolean {
  const w = el.clientWidth;
  const h = el.clientHeight;
  return w >= MIN_SIGMA_SIZE && h >= MIN_SIGMA_SIZE && w <= MAX_CANVAS_DIM && h <= MAX_CANVAS_DIM;
}

/** Apply viewport-bounded fallback when flex layout has not sized the graph wrap yet. */
export function ensureGraphWrapSize(wrap: HTMLElement): void {
  if (isValidCanvasSize(wrap)) return;
  const targetH = clampCanvasDim(window.innerHeight * 0.55);
  wrap.style.flex = "1 1 auto";
  wrap.style.width = "100%";
  wrap.style.minHeight = "280px";
  wrap.style.maxHeight = `${targetH}px`;
  wrap.style.height = "100%";
  wrap.style.overflow = "hidden";
  wrap.style.position = "relative";
}

/** Keep WebGL backing store in sync when flex layout changes size. */
export function observeSigmaResize(
  sigma: { resize: (force?: boolean) => unknown; refresh: () => unknown },
  container: HTMLElement,
): () => void {
  const onResize = debounce(() => {
    if (!isValidCanvasSize(container)) return;
    sigma.resize();
    sigma.refresh();
  }, RESIZE_DEBOUNCE_MS);
  const ro = new ResizeObserver(onResize);
  ro.observe(container);
  return () => ro.disconnect();
}

/**
 * Mount Sigma only after the container has measurable size.
 * Uses ResizeObserver so late layout (e.g. first paint) still triggers init.
 */
export function mountSigmaWhenReady(
  container: HTMLElement,
  mount: () => void | (() => void) | Promise<void | (() => void)>,
): () => void {
  let disposed = false;
  let innerCleanup: (() => void) | undefined;
  let mountGen = 0;
  let ro: ResizeObserver | null = null;

  const tryMount = () => {
    if (disposed || innerCleanup) return;
    if (!isValidCanvasSize(container)) return;
    const gen = ++mountGen;
    void Promise.resolve(mount()).then((result) => {
      if (disposed || gen !== mountGen) return;
      if (typeof result === "function") {
        innerCleanup = result;
      }
    });
  };

  void (async () => {
    await waitForContainerSize(container);
    tryMount();
  })();

  ro = new ResizeObserver(() => tryMount());
  ro.observe(container);

  return () => {
    disposed = true;
    ro?.disconnect();
    innerCleanup?.();
  };
}

export interface SigmaMountHandle {
  sigma: Sigma;
}

/**
 * Mount Sigma inside a bounded graph wrap. The host is absolutely positioned to fill
 * the wrap; never copy unbounded layout sizes onto the WebGL canvas.
 */
export function mountSigmaInWrap(
  wrap: HTMLElement,
  host: HTMLElement,
  create: () => SigmaMountHandle,
): () => void {
  let disposed = false;
  let sigma: Sigma | null = null;
  let ro: ResizeObserver | null = null;

  const tryMount = () => {
    if (disposed || sigma) return;
    ensureGraphWrapSize(wrap);
    if (!isValidCanvasSize(wrap) || !isValidCanvasSize(host)) return;
    const handle = create();
    sigma = handle.sigma;
    sigma.resize();
    sigma.refresh();
  };

  const onResize = debounce(() => {
    if (disposed) return;
    if (!isValidCanvasSize(wrap)) return;
    if (sigma) {
      sigma.resize();
      sigma.refresh();
      return;
    }
    tryMount();
  }, RESIZE_DEBOUNCE_MS);

  void (async () => {
    await waitForContainerSize(wrap);
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
    );
    if (!disposed) tryMount();
  })();

  ro = new ResizeObserver(onResize);
  ro.observe(wrap);

  return () => {
    disposed = true;
    ro?.disconnect();
    sigma?.kill();
    sigma = null;
  };
}
