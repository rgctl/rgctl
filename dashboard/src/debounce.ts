/** Debounce rapid callbacks (e.g. ResizeObserver → sigma.refresh). */
export function debounce<T extends (...args: never[]) => void>(
  fn: T,
  ms: number,
): T {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return ((...args: never[]) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  }) as T;
}
