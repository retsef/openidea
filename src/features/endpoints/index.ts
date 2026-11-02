/**
 * Endpoints tool window (IntelliJ's Endpoints): the project's HTTP endpoints
 * as the registered providers report them (`registerEndpointProvider`; Rails
 * routes in `features/rails`). The window is registered while some provider
 * applies to the project.
 */
import { registerActions } from "@/platform/actions";
import { registerToolWindow, toolWindowService } from "@/workbench/contributions";
import { ENDPOINTS_WINDOW, EndpointsHeaderActions, EndpointsView, useEndpointsHeaderMenu } from "./EndpointsView";
import { refreshEndpoints, useEndpoints } from "./store";

export { registerEndpointProvider, selectEndpoint } from "./store";
export type { Endpoint, EndpointProvider, ProviderState } from "./model";
export { ENDPOINTS_WINDOW };

/** Registers the tool window while a provider applies to the project. */
function installToolWindow(): () => void {
  let dispose: (() => void) | null = null;
  const sync = (present: boolean) => {
    if (present && !dispose) {
      dispose = registerToolWindow({
        id: ENDPOINTS_WINDOW,
        title: "Endpoints",
        icon: "toolwindow-endpoints",
        anchor: "right",
        order: 45,
        component: EndpointsView,
        headerActions: EndpointsHeaderActions,
        headerMenu: useEndpointsHeaderMenu,
      });
    } else if (!present && dispose) {
      dispose();
      dispose = null;
    }
  };
  sync(useEndpoints.getState().available);
  const unsubscribe = useEndpoints.subscribe((s) => sync(s.available));
  return () => {
    unsubscribe();
    dispose?.();
  };
}

export function activate(): () => void {
  const disposers = [
    installToolWindow(),
    registerActions([
      {
        id: "endpoints.show",
        title: "Endpoints",
        category: "Tools",
        icon: "toolwindow-endpoints",
        keywords: ["routes", "rest", "url", "api", "rails"],
        when: "projectOpen",
        run: () => {
          // The window exists only while a provider applies to the project.
          if (useEndpoints.getState().available) toolWindowService().show(ENDPOINTS_WINDOW, true);
        },
      },
      {
        id: "endpoints.refresh",
        title: "Refresh Endpoints",
        category: "Tools",
        icon: "refresh",
        when: "projectOpen",
        run: () => refreshEndpoints(),
      },
    ]),
  ];
  return () => disposers.reverse().forEach((d) => d());
}
