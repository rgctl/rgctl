/**
 * `useFocus` — the Focus as a React hook over the URL.
 *
 * The URL is the store: this hook never keeps a parallel copy of the key. Callers receive the current
 * key, the resolved entry (or `null` when stale / absent), and writers that only mutate search params.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { getRouteApi } from "@tanstack/react-router";
import { fetchFunctionById, loadFunctions, type FunctionEntry, type Manifest } from "./data";
import {
  DEFAULT_ROOM,
  isLens,
  isRoom,
  keyOf,
  type Focus,
  type LensId,
  type RoomId,
} from "./focus";

const roomApi = getRouteApi("/$roomId");

export interface FocusState {
  room: RoomId;
  lens: LensId;
  /** The Focus key from the URL, or `null` when none is set. */
  focus: Focus | null;
  /** Resolved index entry; `null` when there is no Focus or it does not resolve. */
  entry: FunctionEntry | null;
  /** True when the URL carries a Focus the current index does not contain. */
  staleFocus: boolean;
  /** The district open in Architecture, from the URL — so leaving the room and returning keeps the level. */
  explode: number | null;
  functions: FunctionEntry[];
  functionsLoading: boolean;
  functionsError: unknown;
  setFocus: (key: Focus | null) => void;
  setLens: (lens: LensId) => void;
  setExplode: (id: number | null) => void;
  goRoom: (room: RoomId) => void;
  /** Set Focus from an index entry (key only — never the payload). */
  pick: (entry: FunctionEntry) => void;
  /** Set Focus from a stable key, for results (semantic hits) that are not in the local symbol index. */
  pickKey: (key: Focus) => void;
}

export function useFocus(options: {
  /**
   * Whether the full function index is needed on this surface. Architecture can render without it;
   * Repo/Understand/Review and an open palette cannot. A Focus in the URL needs the index to resolve.
   */
  enableFunctions?: boolean | ((room: RoomId, focus: Focus | null) => boolean);
} = {}): FocusState {
  const params = roomApi.useParams();
  const search = roomApi.useSearch();
  const navigate = roomApi.useNavigate();
  const queryClient = useQueryClient();

  const room = (isRoom(params.roomId) ? params.roomId : DEFAULT_ROOM) as RoomId;
  const lens = (isLens(search.lens) ? search.lens : "cfg") as LensId;
  const focus = (search.focus as Focus | undefined) ?? null;
  const explode = (search.explode as number | undefined) ?? null;

  const functionsEnabled =
    typeof options.enableFunctions === "function"
      ? options.enableFunctions(room, focus)
      : (options.enableFunctions ?? true);
  // Prefix the cache with the bundle digest. Waiting for the manifest avoids a "pending" key
  // fetching the 134 MB index and then immediately refetching under the real digest.
  const manifest = queryClient.getQueryData<Manifest | null>(["manifest"]);
  const digest = manifest?.graph?.digest ?? null;
  const functions = useQuery({
    queryKey: ["functions", digest ?? "pending"],
    queryFn: loadFunctions,
    enabled: functionsEnabled && digest !== null,
  });
  const list = functions.data ?? [];
  // O(1) Focus resolution. The palette and Repo still scan the array for text search, but a URL
  // lookup and every lens switch no longer walk 334k rows.
  const byId = useMemo(
    () => new Map(list.map((f) => [f.function_id, f])),
    [list],
  );
  // A semantic hit or a shared link can name a function outside the first directory page. Resolve it
  // through the server instead of loading the whole index into the browser.
  const remoteEntry = useQuery({
    queryKey: ["function", focus, digest],
    queryFn: () => fetchFunctionById(focus as string),
    enabled:
      focus !== null &&
      functionsEnabled &&
      digest !== null &&
      !byId.has(focus),
  });
  const entry = focus ? (byId.get(focus) ?? remoteEntry.data ?? null) : null;
  const staleFocus =
    focus !== null &&
    functionsEnabled &&
    digest !== null &&
    !entry &&
    !functions.isLoading &&
    !remoteEntry.isLoading;

  const setFocus = (key: Focus | null) =>
    void navigate({ search: (prev) => ({ ...prev, focus: key ?? undefined }), replace: false });

  const setLens = (next: LensId) =>
    void navigate({ search: (prev) => ({ ...prev, lens: next }) });

  const setExplode = (id: number | null) =>
    void navigate({ search: (prev) => ({ ...prev, explode: id ?? undefined }) });

  const goRoom = (next: RoomId) =>
    void navigate({ to: "/$roomId", params: { roomId: next }, search: (prev) => prev });

  const pickKey = (key: Focus) => {
    // Rooms that do not read the Focus would take a selection and show nothing with it, leaving the graph
    // two actions away. Architecture is one of them, and it is now the landing room, so ⌘K from `/` has to
    // carry you to the surface that renders what you picked.
    if (room === "repo" || room === "architecture") {
      void navigate({
        to: "/$roomId",
        params: { roomId: "understand" },
        search: (prev) => ({ ...prev, focus: key }),
      });
      return;
    }
    setFocus(key);
  };

  const pick = (f: FunctionEntry) => pickKey(keyOf(f));

  return {
    room,
    lens,
    focus,
    entry,
    staleFocus,
    functions: list,
    functionsLoading: functionsEnabled && (digest === null || functions.isLoading),
    functionsError: functions.error,
    explode,
    setFocus,
    setLens,
    setExplode,
    goRoom,
    pick,
    pickKey,
  };
}
