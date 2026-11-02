/**
 * Rails routes as the Endpoints provider and the routes-file gutter see them:
 * identity, controller class, the routes of a line and the `.http` request of
 * a route. Pure functions over the table `rails_routes` returns.
 */
import type { RailsRoute } from "@/bindings";
import type { RequestSpec } from "../http-client/generate";

/** The default `bin/rails server` address. */
export const RAILS_HOST = "http://localhost:3000";

/** Stable identity of a route row: the defining call plus the route (a line may produce several). */
export function routeKey(r: RailsRoute): string {
  return `${r.file}:${r.line} ${r.verb} ${r.path} ${r.controller}#${r.action}`;
}

/** `controller#action`; the engine constant for a mounted engine. */
export function routeTarget(r: RailsRoute): string {
  return r.action ? `${r.controller}#${r.action}` : r.controller;
}

/** The controller class as IntelliJ's Endpoints shows it: `admin/photos` → `Admin::PhotosController`; engines and redirects as they are. */
export function controllerClass(r: RailsRoute): string {
  if (!r.action || /[A-Z:(]/.test(r.controller)) return r.controller;
  return `${r.controller
    .split("/")
    .map((part) =>
      part
        .split("_")
        .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
        .join(""),
    )
    .join("::")}Controller`;
}

/** `config/routes.rb` or `config/routes/<name>.rb`. */
export function isRoutesFile(path: string): boolean {
  return /(?:^|\/)config\/routes(?:\.rb|\/[^/]+\.rb)$/.test(path);
}

/** Paths whose change invalidates the table: the files read, plus the routes files that may appear. */
export function affectsRoutes(path: string, root: string, files: readonly string[]): boolean {
  if (files.includes(path)) return true;
  return path.startsWith(`${root}/`) && isRoutesFile(path);
}

/** The routes produced by the DSL call on `line` (1-based) of `file`. */
export function routesAt(routes: readonly RailsRoute[], file: string, line: number): RailsRoute[] {
  return routes.filter((r) => r.file === file && r.line === line);
}

/** 1-based lines of `file` that produce routes, ascending. */
export function routeLines(routes: readonly RailsRoute[], file: string): number[] {
  return [...new Set(routes.filter((r) => r.file === file).map((r) => r.line))].sort((a, b) => a - b);
}

/** `/photos/:id` → `/photos/{{id}}`, `*path` → `{{path}}`. */
export function pathVariables(path: string): string {
  return path.replace(/[:*]([A-Za-z_]\w*)/g, "{{$1}}");
}

/** The `.http` request of a route against `host`; `ANY` (match via: :all, engines) is sent as GET. */
export function requestSpec(r: RailsRoute, host = RAILS_HOST): RequestSpec {
  const method = r.verb === "ANY" ? "GET" : r.verb;
  const headers = [{ name: "Accept", value: "application/json" }];
  const withBody = method === "POST" || method === "PUT" || method === "PATCH";
  if (withBody) headers.push({ name: "Content-Type", value: "application/json" });
  return {
    name: routeTarget(r),
    method,
    url: `${host}${pathVariables(r.path)}`,
    headers,
    body: withBody ? "{}" : undefined,
  };
}
