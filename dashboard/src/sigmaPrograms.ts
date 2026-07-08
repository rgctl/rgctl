import { NodeCircleProgram, NodePointProgram } from "sigma/rendering";

/** Sigma v3 defaults only register `circle`; we need `point` for square nodes. */
export const SIGMA_NODE_PROGRAM_CLASSES = {
  circle: NodeCircleProgram,
  point: NodePointProgram,
};
