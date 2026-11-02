/**
 * Ruby on Rails: the Routes tool window (the application's routes read
 * statically from `config/routes.rb`, see `ide-rails`) and, in routes files,
 * a gutter icon per route-producing line that generates `.http` requests.
 * The tool window is registered only while the project has a `config/routes.rb`.
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
import { ROUTES_WINDOW, RoutesHeaderActions, RoutesView, useRoutesHeaderMenu } from "./RoutesView";
import { installRailsRoutes, loadRoutes, useRailsRoutes } from "./store";

const ROUTES_FILE_WHEN = "editorLangId == ruby";

/** Registers the tool window while the project has a route table. */
function installToolWindow(): () => void {
  let dispose: (() => void) | null = null;
  const sync = (present: boolean) => {
    if (present && !dispose) {
      dispose = registerToolWindow({
        id: ROUTES_WINDOW,
        title: "Routes",
        icon: "web",
        anchor: "bottom",
        order: 45,
        component: RoutesView,
        headerActions: RoutesHeaderActions,
        headerMenu: useRoutesHeaderMenu,
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
        id: "rails.showRoutes",
        title: "Routes",
        category: "Tools",
        icon: "web",
        keywords: ["rails", "endpoints", "routes.rb", "url"],
        when: "projectOpen",
        run: () => {
          // The window exists only while the project has a route table.
          if (useRailsRoutes.getState().table !== null) toolWindowService().show(ROUTES_WINDOW, true);
        },
      },
      {
        id: "rails.refreshRoutes",
        title: "Refresh Rails Routes",
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
        title: "Show in Routes",
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
