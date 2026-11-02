import { afterEach, describe, expect, it } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import type { RailsRoute } from "@/bindings";
import { useProject } from "@/platform/project";
import { provideEditorService } from "@/workbench/contributions";
import { RoutesHeaderActions, RoutesView, useRoutesView, useRoutesViewOptions } from "./RoutesView";
import { useRailsRoutes } from "./store";

const ROOT = "/p/app";
const MAIN = `${ROOT}/config/routes.rb`;
const ADMIN = `${ROOT}/config/routes/admin.rb`;

function route(verb: RailsRoute["verb"], path: string, target: string, extra: Partial<RailsRoute> = {}): RailsRoute {
  const [controller, action = ""] = target.split("#");
  return { verb, path, controller: controller!, action, name: null, file: MAIN, line: 1, source: "explicit", ...extra };
}

const TABLE = {
  routes: [
    route("GET", "/photos", "photos#index", { name: "photos", line: 4, source: "resources" }),
    route("GET", "/photos/:id", "photos#show", { name: "photo", line: 4, source: "resources" }),
    route("GET", "/admin/stats", "admin/stats#index", { name: "admin_stats", file: ADMIN, line: 2 }),
  ],
  diagnostics: [{ file: MAIN, line: 12, message: "`draw :missing`: config/routes/missing.rb not found" }],
  files: [MAIN, ADMIN],
};

describe("RoutesView", () => {
  afterEach(() => {
    act(() => {
      useRailsRoutes.setState({ table: null, loading: false, loaded: false, error: null, selectedKey: null });
      useRoutesView.setState({ query: "", collapsed: new Set(), showDiagnostics: false });
      useRoutesViewOptions.setState({ grouped: false, autoscroll: false });
    });
  });

  it("lists the routes as Verb | Path | Controller#action | Helper and filters them", () => {
    act(() => useRailsRoutes.setState({ table: TABLE, loaded: true }));
    render(
      <>
        <RoutesHeaderActions />
        <RoutesView />
      </>,
    );
    expect(screen.getByText("/photos/:id")).toBeInTheDocument();
    expect(screen.getByText("admin/stats#index")).toBeInTheDocument();
    expect(screen.getByText("photo_path")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Filter routes"), { target: { value: "admin" } });
    expect(screen.queryByText("/photos/:id")).toBeNull();
    expect(screen.getByText("/admin/stats")).toBeInTheDocument();
  });

  it("groups by file and opens the defining line on activation", () => {
    const opened: { path: string; line: number }[] = [];
    provideEditorService({
      openFile: async (path: string, o?: { selection?: { line: number } }) =>
        void opened.push({ path, line: o?.selection?.line ?? -1 }),
    } as never);
    act(() => {
      useProject.setState({ project: { root: ROOT, name: "app" } as never });
      useRailsRoutes.setState({ table: TABLE, loaded: true });
      useRoutesViewOptions.setState({ grouped: true });
    });
    render(<RoutesView />);
    expect(screen.getByText("admin.rb")).toBeInTheDocument();
    expect(screen.getByText("1 route")).toBeInTheDocument();
    expect(screen.getByText("2 routes")).toBeInTheDocument();
    fireEvent.doubleClick(screen.getByText("/admin/stats"));
    expect(opened).toEqual([{ path: ADMIN, line: 1 }]);
  });

  it("shows the diagnostics when asked", () => {
    act(() => {
      useProject.setState({ project: { root: ROOT, name: "app" } as never });
      useRailsRoutes.setState({ table: TABLE, loaded: true });
    });
    render(
      <>
        <RoutesHeaderActions />
        <RoutesView />
      </>,
    );
    expect(screen.queryByText(/missing\.rb not found/)).toBeNull();
    fireEvent.click(screen.getByLabelText("1 route could not be read"));
    expect(
      screen.getByText("config/routes.rb:12: `draw :missing`: config/routes/missing.rb not found"),
    ).toBeInTheDocument();
  });

  it("explains an empty table", () => {
    act(() => useRailsRoutes.setState({ table: null, loaded: true }));
    render(<RoutesView />);
    expect(screen.getByText("Not a Rails project")).toBeInTheDocument();
    act(() => useRailsRoutes.setState({ table: { routes: [], diagnostics: [], files: [MAIN] }, loaded: true }));
    expect(screen.getByText("No routes defined")).toBeInTheDocument();
  });
});
