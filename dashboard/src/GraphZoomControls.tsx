import type Sigma from "sigma";

export interface GraphZoomControlsProps {
  sigmaRef: { current: Sigma | null };
  class?: string;
}

export function GraphZoomControls({ sigmaRef, class: className }: GraphZoomControlsProps) {
  const fitView = () => {
    sigmaRef.current?.getCamera().animatedReset({ duration: 300 });
  };

  const zoom = (factor: number) => {
    const cam = sigmaRef.current?.getCamera();
    if (!cam) return;
    cam.animate({ ratio: cam.ratio * factor }, { duration: 200 });
  };

  return (
    <div
      class={["graph-zoom-controls", className].filter(Boolean).join(" ")}
      role="group"
      aria-label="Zoom"
    >
      <button
        type="button"
        class="graph-zoom-btn"
        onClick={() => zoom(0.75)}
        title="Zoom in"
        aria-label="Zoom in"
      >
        +
      </button>
      <button
        type="button"
        class="graph-zoom-btn"
        onClick={fitView}
        title="Fit view"
        aria-label="Fit view"
      >
        ⊡
      </button>
      <button
        type="button"
        class="graph-zoom-btn"
        onClick={() => zoom(1.33)}
        title="Zoom out"
        aria-label="Zoom out"
      >
        −
      </button>
    </div>
  );
}
