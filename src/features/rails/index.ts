/**
 * Ruby on Rails: the Endpoints tool window (the application's routes read
 * statically from `config/routes.rb`, see `ide-rails`, laid out like
 * IntelliJ's Endpoints) and, in routes files, a gutter icon per
 * route-producing line that generates `.http` requests. The tool window is
 * registered only while the project has a `config/routes.rb`.
 */
import { registerEditorContribution } from "@/editor/api";
import { registerActions } from "@/platform/actions";
import { registerMenuGroup, registerToolWindow, toolWindowService } from "@/workbench/contributions";
import {
  GENERATE_REQUEST,
  SHOW_IN_ROUTES,
  generateRequests,
  menuGroupItems,
  railsRoutesEditorExtension,
  routesAtCaret,
  showInRoutes,
} from "./editor";
import { ENDPOINTS_WINDOW, EndpointsHeaderActions, EndpointsView, useEndpointsHeaderMenu } from "./EndpointsView";
import { installRailsRoutes, loadRoutes, useRailsRoutes } from "./store";

const ROUTES_FILE_WHEN = "editorLangId == ruby";

/** Registers the tool window while the project has a route table. */
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
  sync(useRailsRoutes.getState().table !== null);
  const unsubscribe = useRailsRoutes.subscribe((s) => sync(s.table !== null));
  return () => {
    unsubscribe();
    dispose?.();
  };
}

export function activate(): () => void {
  const disposers = [
    installRailsRoutes(),
    installToolWindow(),
    registerEditorContribution({ id: "rails.routes", order: 58, extensions: railsRoutesEditorExtension }),
    registerMenuGroup({ id: "rails.routes", menus: ["editor"], order: 41, items: (_menu, ctx) => menuGroupItems(ctx) }),
    registerActions([
      {
        id: "endpoints.show",
        title: "Endpoints",
        category: "Tools",
        icon: "toolwindow-endpoints",
        keywords: ["rails", "routes", "routes.rb", "url", "rest"],
        when: "projectOpen",
        run: () => {
          // The window exists only while the project has a route table.
          if (useRailsRoutes.getState().table !== null) toolWindowService().show(ENDPOINTS_WINDOW, true);
        },
      },
      {
        id: "endpoints.refresh",
        title: "Refresh Endpoints",
        category: "Tools",
        icon: "refresh",
        when: "projectOpen",
        run: () => loadRoutes(),
      },
      {
        id: GENERATE_REQUEST,
        title: "Generate Request in HTTP Client",
        category: "Tools",
        icon: "file-http",
        keywords: ["rails", "routes", "rest", "endpoint", "http client"],
        when: ROUTES_FILE_WHEN,
        run: () => generateRequests(routesAtCaret()),
      },
      {
        id: SHOW_IN_ROUTES,
        title: "Show in Endpoints",
        category: "Tools",
        icon: "web",
        keywords: ["rails", "routes"],
        when: ROUTES_FILE_WHEN,
        run: () => {
          const route = routesAtCaret()[0];
          if (route) showInRoutes(route);
        },
      },
    ]),
  ];
  return () => disposers.reverse().forEach((d) => d());
}
