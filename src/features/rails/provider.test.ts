import { afterEach, describe, expect, it } from "vitest";
import type { RailsRoute } from "@/bindings";
import { useProject } from "@/platform/project";
import { endpointId, railsEndpointProvider, toEndpoint } from "./provider";
import { useRailsRoutes } from "./store";

const ROOT = "/p/app";
const MAIN = `${ROOT}/config/routes.rb`;

function route(verb: RailsRoute["verb"], path: string, target: string, extra: Partial<RailsRoute> = {}): RailsRoute {
  const [controller, action = ""] = target.split("#");
  return { verb, path, controller: controller!, action, name: null, file: MAIN, line: 1, source: "explicit", ...extra };
}

describe("Rails endpoint provider", () => {
  afterEach(() => {
    useRailsRoutes.setState({ table: null, loading: false, loaded: false, error: null });
  });

  it("maps a route to an endpoint", () => {
    const e = toEndpoint(route("GET", "/photos/:id", "admin/photos#show", { name: "admin_photo", line: 7 }), "app");
    expect(e).toMatchObject({
      id: endpointId(route("GET", "/photos/:id", "admin/photos#show", { line: 7 })),
      method: "GET",
      path: "/photos/:id",
      target: "admin/photos#show",
      targetClass: "Admin::PhotosController",
      name: "admin_photo",
      file: MAIN,
      line: 7,
      module: "app",
      framework: "Rails",
      type: "HTTP",
    });
    expect(e.request.url).toBe("http://localhost:3000/photos/{{id}}");
    expect(e.docs).toContainEqual(["Helper", "admin_photo_path / admin_photo_url"]);
    expect(e.docs).toContainEqual(["Defined by", "route"]);
  });

  it("reports no endpoints for a project without routes, and the table's state otherwise", () => {
    useProject.setState({ project: { root: ROOT, name: "app" } as never });
    const p = railsEndpointProvider();
    expect(p.getState()).toMatchObject({ endpoints: null, loaded: false });
    const table = { routes: [route("GET", "/up", "rails/health#show")], diagnostics: [], files: [MAIN] };
    useRailsRoutes.setState({ table, loaded: true });
    const state = p.getState();
    expect(state.loaded).toBe(true);
    expect(state.endpoints?.map((e) => [e.path, e.targetClass, e.module])).toEqual([
      ["/up", "Rails::HealthController", "app"],
    ]);
    // Mapped once per table.
    expect(p.getState().endpoints).toBe(state.endpoints);
    useRailsRoutes.setState({ loading: true });
    expect(p.getState()).toMatchObject({ loading: true });
    expect(p.getState().endpoints).toBe(state.endpoints);
  });
});
