import {
  createHashHistory,
  createRootRoute,
  createRoute,
  createRouter,
} from "@tanstack/react-router";
import { StudioShell } from "./studio-shell/StudioShell";
import { StudioPage } from "./routes/studio";
import { SettingsPage } from "./routes/settings";

// The Beesoft Studio shell is the app root: it owns the browser-style tab strip,
// the homepage + dubbing surface (rendered as its own overlays), and hosts the
// remaining feature pages (TTS / Settings) via <Outlet/>.
const rootRoute = createRootRoute({ component: StudioShell });

const studioRoute = createRoute({ getParentRoute: () => rootRoute, path: "/", component: StudioPage });
const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: SettingsPage,
});

const routeTree = rootRoute.addChildren([studioRoute, settingsRoute]);

export const router = createRouter({
  routeTree,
  history: createHashHistory(),
  defaultPreload: "intent",
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
