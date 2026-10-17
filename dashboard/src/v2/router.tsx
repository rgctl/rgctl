import {
  createRootRoute,
  createRoute,
  createRouter,
  redirect,
  Outlet,
  type RouterHistory,
} from "@tanstack/react-router";
import { isLens, isRoom, parseFocus } from "./focus";
import { App } from "./App";

/** Search params are the Focus's transport: a view state is a shareable link. */
export interface ViewSearch {
  lens?: string;
  focus?: string;
}

const rootRoute = createRootRoute({ component: () => <Outlet /> });

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/$roomId", params: { roomId: "repo" }, replace: true });
  },
});

/**
 * One route, `$roomId`, rather than one route per room: rooms are destinations that differ by data,
 * not by mounting a different application. Lenses and the Focus are search params, which encodes the
 * IA's own distinction in the URL — a room is somewhere you go, a lens is a control over what you are
 * looking at.
 */
const roomRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "$roomId",
  component: App,
  beforeLoad: ({ params }) => {
    // An unknown room is normalised rather than rendered as a broken shell with a lying URL.
    if (!isRoom(params.roomId)) {
      throw redirect({ to: "/$roomId", params: { roomId: "repo" }, replace: true });
    }
  },
  validateSearch: (search: Record<string, unknown>): ViewSearch => ({
    lens: isLens(search.lens) ? (search.lens as string) : undefined,
    focus: parseFocus(search.focus) ?? undefined,
  }),
});

const routeTree = rootRoute.addChildren([indexRoute, roomRoute]);

/**
 * A factory, so tests can drive the real router with a memory history.
 *
 * The alternative — a second route tree written for tests — would be a copy that drifts, and the thing
 * most worth testing here is precisely the routing contract (`validateSearch` normalising an unknown lens,
 * room preservation across a lens switch). A test against a copy proves nothing about the original.
 */
export function createAppRouter(history?: RouterHistory) {
  return createRouter({ routeTree, ...(history ? { history } : {}), defaultPreload: false });
}

export const router = createAppRouter();

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
