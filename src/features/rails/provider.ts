/**
 * Rails routes as endpoints of the Endpoints tool window: the route table of
 * `store.ts` mapped to the neutral model (`features/endpoints`), re-mapped
 * only when the table changes.
 */
import type { RailsRoute, RailsRouteTable } from "@/bindings";
import { useProject } from "@/platform/project";
import type { Endpoint, EndpointProvider, ProviderState } from "../endpoints";
import { controllerClass, requestSpec, routeKey, routeTarget } from "./model";
import { loadRoutes, useRailsRoutes } from "./store";

export const RAILS_PROVIDER = "rails";
export const RAILS_FRAMEWORK = "Rails";

/** The endpoint id of a route (`selectEndpoint`). */
export function endpointId(route: RailsRoute): string {
  return `${RAILS_PROVIDER}:${routeKey(route)}`;
}

const SOURCE_LABEL: Record<RailsRoute["source"], string> = {
  explicit: "route",
  resources: "resources",
  concern: "concern",
};

export function toEndpoint(route: RailsRoute, module: string): Endpoint {
  return {
    id: endpointId(route),
    method: route.verb,
    path: route.path,
    target: routeTarget(route),
    targetClass: controllerClass(route),
    name: route.name,
    file: route.file,
    line: route.line,
    module,
    framework: RAILS_FRAMEWORK,
    type: "HTTP",
    icon: "web",
    request: requestSpec(route),
    docs: [
      ["Controller", controllerClass(route)],
      ["Action", route.action || "—"],
      ["Helper", route.name ? `${route.name}_path / ${route.name}_url` : "—"],
      ["Defined by", SOURCE_LABEL[route.source]],
    ],
  };
}

/** Maps the table once per table / module; the store's other fields pass through. */
export function railsEndpointProvider(): EndpointProvider {
  let mapped: { table: RailsRouteTable | null; module: string; state: ProviderState } | null = null;
  const getState = (): ProviderState => {
    const s = useRailsRoutes.getState();
    const module = useProject.getState().project?.name ?? "";
    if (!mapped || mapped.table !== s.table || mapped.module !== module) {
      mapped = {
        table: s.table,
        module,
        state: {
          endpoints: s.table ? s.table.routes.map((r) => toEndpoint(r, module)) : null,
          loading: s.loading,
          loaded: s.loaded,
          error: s.error,
          diagnostics: s.table?.diagnostics ?? [],
        },
      };
    } else if (
      mapped.state.loading !== s.loading ||
      mapped.state.loaded !== s.loaded ||
      mapped.state.error !== s.error
    ) {
      mapped = { ...mapped, state: { ...mapped.state, loading: s.loading, loaded: s.loaded, error: s.error } };
    }
    return mapped.state;
  };
  return {
    id: RAILS_PROVIDER,
    framework: RAILS_FRAMEWORK,
    type: "HTTP",
    getState,
    subscribe: (listener) => {
      const a = useRailsRoutes.subscribe(listener);
      const b = useProject.subscribe((s, prev) => {
        if (s.project?.name !== prev.project?.name) listener();
      });
      return () => {
        a();
        b();
      };
    },
    refresh: loadRoutes,
  };
}
