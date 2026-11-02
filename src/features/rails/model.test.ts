import { describe, expect, it } from "vitest";
import type { RailsRoute, RailsRouteTable } from "@/bindings";
import { renderRequestBlock } from "../http-client/generate";
import {
  affectsRoutes,
  diagnosticLines,
  filterRoutes,
  groupByFile,
  isRoutesFile,
  navigationTarget,
  pathVariables,
  requestSpec,
  routeKey,
  routeLines,
  routesAt,
} from "./model";

const ROOT = "/p/app";
const MAIN = `${ROOT}/config/routes.rb`;
const ADMIN = `${ROOT}/config/routes/admin.rb`;

function route(verb: RailsRoute["verb"], path: string, target: string, extra: Partial<RailsRoute> = {}): RailsRoute {
  const [controller, action = ""] = target.split("#");
  return { verb, path, controller: controller!, action, name: null, file: MAIN, line: 1, source: "explicit", ...extra };
}

const ROUTES: RailsRoute[] = [
  route("GET", "/photos", "photos#index", { name: "photos", line: 4, source: "resources" }),
  route("POST", "/photos", "photos#create", { line: 4, source: "resources" }),
  route("GET", "/photos/:id", "photos#show", { name: "photo", line: 4, source: "resources" }),
  route("GET", "/admin/stats", "admin/stats#index", { name: "admin_stats", file: ADMIN, line: 2 }),
  route("ANY", "/sidekiq", "Sidekiq::Web", { name: "sidekiq_web", line: 9 }),
];

describe("routes model", () => {
  it("recognises routes files", () => {
    expect(isRoutesFile(MAIN)).toBe(true);
    expect(isRoutesFile(ADMIN)).toBe(true);
    expect(isRoutesFile(`${ROOT}/config/routes/api/v1.rb`)).toBe(false);
    expect(isRoutesFile(`${ROOT}/app/controllers/photos_controller.rb`)).toBe(false);
    expect(isRoutesFile(`${ROOT}/config/routes.rb.bak`)).toBe(false);
  });

  it("reloads on the files read and on routes files appearing in the project", () => {
    const files = [MAIN, ADMIN];
    expect(affectsRoutes(MAIN, ROOT, files)).toBe(true);
    expect(affectsRoutes(`${ROOT}/config/routes/new.rb`, ROOT, files)).toBe(true);
    expect(affectsRoutes(`${ROOT}/app/models/photo.rb`, ROOT, files)).toBe(false);
    expect(affectsRoutes("/elsewhere/config/routes.rb", ROOT, files)).toBe(false);
  });

  it("filters by every word, case-insensitively, over verb, path, target and helper", () => {
    expect(filterRoutes(ROUTES, "").length).toBe(5);
    expect(filterRoutes(ROUTES, "post").map((r) => r.path)).toEqual(["/photos"]);
    expect(filterRoutes(ROUTES, "GET photo").map((r) => r.path)).toEqual(["/photos", "/photos/:id"]);
    expect(filterRoutes(ROUTES, "admin_stats").map((r) => r.path)).toEqual(["/admin/stats"]);
    expect(filterRoutes(ROUTES, "sidekiq::web").map((r) => r.verb)).toEqual(["ANY"]);
    expect(filterRoutes(ROUTES, "nothing here")).toEqual([]);
  });

  it("groups by file in first-use order", () => {
    const groups = groupByFile(ROUTES);
    expect(groups.map((g) => [g.file, g.routes.length])).toEqual([
      [MAIN, 4],
      [ADMIN, 1],
    ]);
  });

  it("navigates to the DSL call, 0-based", () => {
    expect(navigationTarget(ROUTES[3]!)).toEqual({ path: ADMIN, line: 1, col: 0 });
    expect(navigationTarget(route("GET", "/", "a#b", { line: 0 }))).toEqual({ path: MAIN, line: 0, col: 0 });
  });

  it("finds the routes of a line and the lines with routes", () => {
    expect(routesAt(ROUTES, MAIN, 4).map(routeKey)).toEqual([
      `${MAIN}:4 GET /photos photos#index`,
      `${MAIN}:4 POST /photos photos#create`,
      `${MAIN}:4 GET /photos/:id photos#show`,
    ]);
    expect(routesAt(ROUTES, ADMIN, 4)).toEqual([]);
    expect(routeLines(ROUTES, MAIN)).toEqual([4, 9]);
    expect(routeLines(ROUTES, ADMIN)).toEqual([2]);
  });

  it("renders a route as an .http request with path variables", () => {
    expect(pathVariables("/photos/:photo_id/comments/:id")).toBe("/photos/{{photo_id}}/comments/{{id}}");
    expect(pathVariables("/legacy/*path")).toBe("/legacy/{{path}}");
    expect(renderRequestBlock(requestSpec(ROUTES[2]!))).toBe(
      "### photos#show\nGET http://localhost:3000/photos/{{id}}\nAccept: application/json\n",
    );
    expect(renderRequestBlock(requestSpec(ROUTES[1]!, "http://localhost:4000"))).toBe(
      "### photos#create\nPOST http://localhost:4000/photos\nAccept: application/json\nContent-Type: application/json\n\n{}\n",
    );
    // Engines and `via: :all` are sent as GET.
    expect(requestSpec(ROUTES[4]!)).toMatchObject({ name: "Sidekiq::Web", method: "GET" });
  });

  it("formats diagnostics relative to the project", () => {
    const table: RailsRouteTable = {
      routes: [],
      files: [MAIN],
      diagnostics: [{ file: MAIN, line: 12, message: "`draw :missing`: config/routes/missing.rb not found" }],
    };
    expect(diagnosticLines(table, ROOT)).toEqual([
      "config/routes.rb:12: `draw :missing`: config/routes/missing.rb not found",
    ]);
  });
});
