/**
 * Ruby on Rails: the Routes tool window (the application's routes read
 * statically from `config/routes.rb`, see `ide-rails`). The tool window is
 * registered only while the project has a `config/routes.rb`.
 */
import { registerActions } from "@/platform/actions";
import { registerToolWindow, toolWindowService } from "@/workbench/contributions";
import { ROUTES_WINDOW, RoutesHeaderActions, RoutesView, useRoutesHeaderMenu } from "./RoutesView";
import { installRailsRoutes, loadRoutes, useRailsRoutes } from "./store";

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
    registerActions([
      {
        id: "rails.showRoutes",
        title: "Routes",
        category: "Tools",
        icon: "web",
        keywords: ["rails", "endpoints", "routes.rb", "url"],
        when: "projectOpen",
        run: () => toolWindowService().show(ROUTES_WINDOW, true),
      },
      {
        id: "rails.refreshRoutes",
        title: "Refresh Rails Routes",
        category: "Tools",
        icon: "refresh",
        when: "projectOpen",
        run: () => loadRoutes(),
      },
    ]),
  ];
  return () => disposers.reverse().forEach((d) => d());
}
