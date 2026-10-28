/**
 * `useFocus` — the Focus as a React hook over the URL.
 *
 * The URL is the store: this hook never keeps a parallel copy of the key. Callers receive the current
 * key, the resolved entry (or `null` when stale / absent), and writers that only mutate search params.
 */
import { useQuery } from "@tanstack/react-query";
import { getRouteApi } from "@tanstack/react-router";
import { loadFunctions, type FunctionEntry } from "./data";
import { isLens, isRoom, keyOf, resolveFocus, type Focus, type LensId, type RoomId } from "./focus";

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
  functions: FunctionEntry[];
  functionsLoading: boolean;
  functionsError: unknown;
  setFocus: (key: Focus | null) => void;
  setLens: (lens: LensId) => void;
  goRoom: (room: RoomId) => void;
  /** Set Focus from an index entry (key only — never the payload). */
  pick: (entry: FunctionEntry) => void;
}

export function useFocus(): FocusState {
  const params = roomApi.useParams();
  const search = roomApi.useSearch();
  const navigate = roomApi.useNavigate();

  const room = (isRoom(params.roomId) ? params.roomId : "repo") as RoomId;
  const lens = (isLens(search.lens) ? search.lens : "spine") as LensId;
  const focus = (search.focus as Focus | undefined) ?? null;

  const functions = useQuery({ queryKey: ["functions"], queryFn: loadFunctions });
  const list = functions.data ?? [];
  const entry = focus ? resolveFocus(focus, list) : null;
  const staleFocus = focus !== null && !entry && !functions.isLoading;

  const setFocus = (key: Focus | null) =>
    void navigate({ search: (prev) => ({ ...prev, focus: key ?? undefined }), replace: false });

  const setLens = (next: LensId) =>
    void navigate({ search: (prev) => ({ ...prev, lens: next }) });

  const goRoom = (next: RoomId) =>
    void navigate({ to: "/$roomId", params: { roomId: next }, search: (prev) => prev });

  const pick = (f: FunctionEntry) => {
    const key = keyOf(f);
    if (room === "repo") {
      void navigate({
        to: "/$roomId",
        params: { roomId: "understand" },
        search: (prev) => ({ ...prev, focus: key }),
      });
      return;
    }
    setFocus(key);
  };

  return {
    room,
    lens,
    focus,
    entry,
    staleFocus,
    functions: list,
    functionsLoading: functions.isLoading,
    functionsError: functions.error,
    setFocus,
    setLens,
    goRoom,
    pick,
  };
}
