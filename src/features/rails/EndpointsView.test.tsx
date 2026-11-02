import { afterEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import type { RailsRoute } from "@/bindings";
import { useProject } from "@/platform/project";
import { provideEditorService } from "@/workbench/contributions";
import { EndpointsHeaderActions, EndpointsView, useEndpointsView, useEndpointsViewOptions } from "./EndpointsView";
import { useRailsRoutes } from "./store";

const run = vi.hoisted(() => vi.fn(async () => true));
vi.mock("../http-client/store", () => ({ runRequest: run }));

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

function setUp(table: typeof TABLE | null = TABLE) {
  act(() => {
    useProject.setState({ project: { root: ROOT, name: "app" } as never });
    useRailsRoutes.setState({ table, loaded: true });
  });
}

describe("EndpointsView", () => {
  afterEach(() => {
    act(() => {
      useRailsRoutes.setState({ table: null, loading: false, loaded: false, error: null, selectedKey: null });
      useEndpointsView.setState({
        query: "",
        hiddenModules: new Set(),
        hiddenTypes: new Set(),
        hiddenFrameworks: new Set(),
        collapsed: new Set(),
        showDiagnostics: false,
        tab: "http",
        current: null,
      });
      useEndpointsViewOptions.setState({ groupByFile: false, autoscroll: false, details: true });
    });
    run.mockClear();
  });

  it("lists the endpoints under the module as `/path [VERB]` with the controller class, and searches them", () => {
    setUp();
    render(<EndpointsView />);
    expect(screen.getByText("app")).toBeInTheDocument();
    expect(screen.getByText("/photos/:id")).toBeInTheDocument();
    expect(screen.getAllByText("[GET]")).toHaveLength(3);
    expect(screen.getByText("Admin::StatsController")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Search endpoints"), { target: { value: "admin" } });
    expect(screen.queryByText("/photos/:id")).toBeNull();
    expect(screen.getByText("/admin/stats")).toBeInTheDocument();
  });

  it("hides everything when the module is unchecked in the Module filter", () => {
    setUp();
    render(<EndpointsView />);
    fireEvent.click(screen.getByRole("button", { name: "Module" }));
    fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "app" }));
    expect(screen.queryByText("/photos/:id")).toBeNull();
    expect(screen.getByText("No endpoints match the filters")).toBeInTheDocument();
  });

  it("shows the selected endpoint's request in the HTTP Client tab and submits it", () => {
    setUp();
    render(<EndpointsView />);
    expect(screen.getByText("Select an endpoint")).toBeInTheDocument();
    fireEvent.mouseDown(screen.getByText("/photos/:id"), { button: 0 });
    expect(screen.getByRole("region", { name: "HTTP request" })).toHaveTextContent(
      "GET http://localhost:3000/photos/{{id}}",
    );
    // The gutter ▶ and the button below share the name; the button carries the text.
    fireEvent.click(screen.getByText("Submit Request"));
    expect(run).toHaveBeenCalledWith(null, expect.stringContaining("GET http://localhost:3000/photos/{{id}}"), 0);
    fireEvent.click(screen.getByRole("tab", { name: "Documentation" }));
    expect(screen.getByText("photo_path / photo_url")).toBeInTheDocument();
    expect(screen.getByText("config/routes.rb:4")).toBeInTheDocument();
  });

  it("groups by file and opens the defining line on activation", () => {
    const opened: { path: string; line: number }[] = [];
    provideEditorService({
      openFile: async (path: string, o?: { selection?: { line: number } }) =>
        void opened.push({ path, line: o?.selection?.line ?? -1 }),
    } as never);
    setUp();
    act(() => useEndpointsViewOptions.setState({ groupByFile: true }));
    render(<EndpointsView />);
    expect(screen.getByText("admin.rb")).toBeInTheDocument();
    expect(screen.getByText("1 route")).toBeInTheDocument();
    fireEvent.doubleClick(screen.getByText("/admin/stats"));
    expect(opened).toEqual([{ path: ADMIN, line: 1 }]);
  });

  it("shows the diagnostics when asked", () => {
    setUp();
    render(
      <>
        <EndpointsHeaderActions />
        <EndpointsView />
      </>,
    );
    expect(screen.queryByText(/missing\.rb not found/)).toBeNull();
    fireEvent.click(screen.getByLabelText("1 route could not be read"));
    expect(
      screen.getByText("config/routes.rb:12: `draw :missing`: config/routes/missing.rb not found"),
    ).toBeInTheDocument();
  });

  it("explains an empty table", () => {
    setUp(null);
    render(<EndpointsView />);
    expect(screen.getByText("Not a Rails project")).toBeInTheDocument();
    act(() => useRailsRoutes.setState({ table: { routes: [], diagnostics: [], files: [MAIN] }, loaded: true }));
    expect(screen.getByText("No endpoints found")).toBeInTheDocument();
  });
});
