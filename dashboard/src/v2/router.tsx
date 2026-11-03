import {
  createRootRoute,
  createRoute,
  createRouter,
  redirect,
  Outlet,
  type RouterHistory,
} from "@tanstack/react-router";
import { isLens, isRoom, parseExplode, parseFocus, DEFAULT_ROOM } from "./focus";
import { App } from "./App";

/** Search params are the Focus's transport: a view state is a shareable link. */
export interface ViewSearch {
  lens?: string;
  focus?: string;
  /** The district whose level is open in Architecture — a level is a link, and a way back to it. */
  explode?: number;
}

const rootRoute = createRootRoute({ component: () => <Outlet /> });

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  beforeLoad: ({ search }) => {
    // `search` rides along: a bare `/` carrying a Focus is still a link someone meant to share.
    throw redirect({
      to: "/$roomId",
      params: { roomId: DEFAULT_ROOM },
      search: search as Record<string, unknown>,
      replace: true,
    });
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
  beforeLoad: ({ params, search }) => {
    // An unknown room is normalised rather than rendered as a broken shell with a lying URL — and the view state
    // comes with it. Dropping `search` here made `/nonsense?focus=…` a link to *no* subject: the room changed and
    // the Focus silently went with it, which is the failure a URL-carrying design exists to prevent.
    if (!isRoom(params.roomId)) {
      throw redirect({
        to: "/$roomId",
        params: { roomId: DEFAULT_ROOM },
        search: search as Record<string, unknown>,
        replace: true,
      });
    }
  },
  validateSearch: (search: Record<string, unknown>): ViewSearch => ({
    lens: isLens(search.lens) ? (search.lens as string) : undefined,
    focus: parseFocus(search.focus) ?? undefined,
    explode: parseExplode(search.explode),
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
