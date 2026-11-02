/**
 * Rails routes as the Routes tool window and the routes-file gutter see them:
 * filtering, grouping by file, navigation targets and the `.http` request of
 * a route. Pure functions over the table `rails_routes` returns.
 */
import type { RailsRoute, RailsRouteTable } from "@/bindings";
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

/** `config/routes.rb` or `config/routes/<name>.rb`. */
export function isRoutesFile(path: string): boolean {
  return /(?:^|\/)config\/routes(?:\.rb|\/[^/]+\.rb)$/.test(path);
}

/** Paths whose change invalidates the table: the files read, plus the routes files that may appear. */
export function affectsRoutes(path: string, root: string, files: readonly string[]): boolean {
  if (files.includes(path)) return true;
  return path.startsWith(`${root}/`) && isRoutesFile(path);
}

/** Case-insensitive match of every whitespace-separated word of `query` against verb, path, target and helper. */
export function filterRoutes(routes: readonly RailsRoute[], query: string): RailsRoute[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return [...routes];
  return routes.filter((r) => {
    const text = `${r.verb} ${r.path} ${routeTarget(r)} ${r.name ?? ""}`.toLowerCase();
    return words.every((w) => text.includes(w));
  });
}

export interface RouteGroup {
  file: string;
  routes: RailsRoute[];
}

/** Routes per file, in the order the files are first used (the main file first). */
export function groupByFile(routes: readonly RailsRoute[]): RouteGroup[] {
  const groups = new Map<string, RailsRoute[]>();
  for (const r of routes) {
    const list = groups.get(r.file);
    if (list) list.push(r);
    else groups.set(r.file, [r]);
  }
  return [...groups].map(([file, list]) => ({ file, routes: list }));
}

/** Where "jump to source" goes: the DSL call that produced the route (0-based editor line). */
export function navigationTarget(r: RailsRoute): { path: string; line: number; col: number } {
  return { path: r.file, line: Math.max(0, r.line - 1), col: 0 };
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

/** Diagnostics for display: `config/routes.rb:12: message`. */
export function diagnosticLines(table: RailsRouteTable, root: string): string[] {
  return table.diagnostics.map((d) => `${relativeTo(root, d.file)}:${d.line}: ${d.message}`);
}

export function relativeTo(root: string, path: string): string {
  return path.startsWith(`${root}/`) ? path.slice(root.length + 1) : path;
}
