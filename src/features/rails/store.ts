/**
 * The project's Rails route table: read through `rails_routes` when a project
 * opens, re-read (debounced) when a routes file is saved or changes on disk.
 * `table` is null while the project has no `config/routes.rb`.
 */
import { create } from "zustand";
import type { RailsRouteTable } from "@/bindings";
import { onEditorEvent } from "@/editor/api";
import { call, commands, isTauri, onIdeEvent } from "@/platform/ipc";
import { useProject } from "@/platform/project";
import { affectsRoutes } from "./model";

interface RoutesState {
  table: RailsRouteTable | null;
  loading: boolean;
  /** The last read failed (the table keeps the previous result). */
  error: string | null;
  /** `rails_routes` answered at least once for this project. */
  loaded: boolean;
}

export const useRailsRoutes = create<RoutesState>(() => ({
  table: null,
  loading: false,
  error: null,
  loaded: false,
}));

let seq = 0;

/** Re-reads the routes files; the newest read wins. */
export async function loadRoutes(): Promise<void> {
  const root = useProject.getState().project?.root;
  if (!root || !isTauri()) {
    useRailsRoutes.setState({ table: null, loading: false, loaded: true, error: null });
    return;
  }
  const mine = ++seq;
  useRailsRoutes.setState({ loading: true });
  try {
    const table = await call(commands.railsRoutes());
    if (mine !== seq) return;
    useRailsRoutes.setState({ table, loading: false, loaded: true, error: null });
  } catch (e) {
    if (mine !== seq) return;
    useRailsRoutes.setState({ loading: false, loaded: true, error: e instanceof Error ? e.message : String(e) });
  }
}

const RELOAD_DELAY = 300;

/** Wires project open / routes-file changes to the table; returns a disposer. */
export function installRailsRoutes(): () => void {
  let root = useProject.getState().project?.root ?? null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  const schedule = (delay: number) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      void loadRoutes();
    }, delay);
  };
  if (root) schedule(500);
  const changed = (paths: string[]) => {
    if (!root) return;
    const files = useRailsRoutes.getState().table?.files ?? [];
    if (paths.some((p) => affectsRoutes(p, root!, files))) schedule(RELOAD_DELAY);
  };
  const unsubProject = useProject.subscribe((s) => {
    const r = s.project?.root ?? null;
    if (r === root) return;
    root = r;
    seq++;
    if (timer) clearTimeout(timer);
    timer = null;
    useRailsRoutes.setState({ table: null, loading: false, loaded: false, error: null });
    if (r) schedule(500);
  });
  const unsubFs = onIdeEvent("fsChanged", (e) => changed([...e.created, ...e.modified, ...e.removed]));
  const unsubEditor = onEditorEvent((e) => {
    if (e.type === "docSaved") changed([e.path]);
  });
  return () => {
    if (timer) clearTimeout(timer);
    unsubProject();
    unsubFs();
    unsubEditor();
  };
}
