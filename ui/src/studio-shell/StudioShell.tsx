import { useCallback, useEffect, useMemo, useState } from "react";
import { Outlet, useNavigate } from "@tanstack/react-router";
import { C, FONT, injectStudioStyles } from "./theme";
import { Icon } from "./icons";
import { HoverBox } from "./ui";
import { Home, type RecentItem } from "./Home";
import { DubList, type DubListItem } from "./DubList";
import { Onboarding } from "./Onboarding";
import { VideoStudio } from "./videostudio/VideoStudio";
import { FEATURE_ICON, FEATURE_TITLE, routeFor, type FeatureKey, type Tab } from "./tabs";
import { createDubProject, listDubProjects, type DubProject } from "../studioApi";
import { ffmpegStatus, isTauri, mediaAiBase, pickVideoFile, serverBase, studioBase } from "../platform";

let UID = 0;
type Gate = "checking" | "onboard" | "app";

export function StudioShell() {
  const navigate = useNavigate();
  const [tabs, setTabs] = useState<Tab[]>([{ id: "home", kind: "home" }]);
  const [activeId, setActiveId] = useState("home");
  const [projects, setProjects] = useState<DubProject[]>([]);
  // Show onboarding whenever the app isn't fully ready (ffmpeg + the 3 sidecars
  // up) — not just on first launch. A ready app skips straight in; a still-
  // downloading or partially-up one gets guided. (Web build: always "app".)
  const [gate, setGate] = useState<Gate>(() => (isTauri() ? "checking" : "app"));
  useEffect(() => {
    if (!isTauri()) return;
    let alive = true;
    const ping = async (u: string | null) => {
      if (!u) return false;
      try {
        return (await fetch(u, { cache: "no-store" })).ok;
      } catch {
        return false;
      }
    };
    void (async () => {
      const [ff, t, s] = await Promise.all([ffmpegStatus(), serverBase(), studioBase()]);
      const [tts, studio, media] = await Promise.all([
        ping(t ? `${t}/health` : null),
        ping(s ? `${s}/health` : null),
        ping(`${mediaAiBase()}/health`),
      ]);
      if (alive) setGate(!!ff?.available && tts && studio && media ? "app" : "onboard");
    })();
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => injectStudioStyles(), []);

  const refresh = useCallback(async () => {
    try {
      setProjects(await listDubProjects());
    } catch {
      /* server may still be starting */
    }
  }, []);
  useEffect(() => {
    void refresh();
  }, [refresh]);

  const active = tabs.find((t) => t.id === activeId) ?? tabs[0];

  const goRoute = useCallback(
    (key: FeatureKey) => {
      const route = routeFor(key);
      if (route) void navigate({ to: route });
    },
    [navigate],
  );

  const activate = useCallback(
    (t: Tab) => {
      setActiveId(t.id);
      if (t.kind !== "home" && t.kind !== "dub" && t.kind !== "project") goRoute(t.kind);
      if (t.kind === "dub") void refresh();
    },
    [goRoute, refresh],
  );

  const openFeature = useCallback(
    (key: FeatureKey) => {
      const existing = tabs.find((t) => t.kind === key);
      if (existing) {
        activate(existing);
        return;
      }
      const tab: Tab = { id: "f" + ++UID, kind: key, title: FEATURE_TITLE[key] };
      setTabs((ts) => [...ts, tab]);
      setActiveId(tab.id);
      if (key === "dub") void refresh();
      else goRoute(key);
    },
    [tabs, activate, goRoute, refresh],
  );

  const openProject = useCallback(
    (id: string) => {
      const existing = tabs.find((t) => t.kind === "project" && t.projectId === id);
      if (existing) {
        setActiveId(existing.id);
        return;
      }
      const p = projects.find((x) => x.id === id);
      const tab: Tab = { id: "p" + ++UID, kind: "project", projectId: id, title: p?.name ?? "Dự án" };
      setTabs((ts) => [...ts, tab]);
      setActiveId(tab.id);
    },
    [tabs, projects],
  );

  const newProject = useCallback(async () => {
    const path = await pickVideoFile();
    if (!path) return;
    const name = path.split(/[/\\]/).pop()?.replace(/\.[^.]+$/, "") ?? "Dự án";
    const p = await createDubProject(name, path);
    await refresh();
    const tab: Tab = { id: "p" + ++UID, kind: "project", projectId: p.id, title: p.name };
    setTabs((ts) => [...ts, tab]);
    setActiveId(tab.id);
  }, [refresh]);

  const closeTab = useCallback(
    (id: string) => (e: React.MouseEvent) => {
      e.stopPropagation();
      setTabs((ts) => {
        const idx = ts.findIndex((t) => t.id === id);
        const next = ts.filter((t) => t.id !== id);
        if (id === activeId) {
          const n = next[idx - 1] ?? next[idx] ?? next[0];
          setActiveId(n ? n.id : "home");
          if (n && n.kind !== "home" && n.kind !== "dub" && n.kind !== "project") goRoute(n.kind);
        }
        return next;
      });
    },
    [activeId, goRoute],
  );

  const recents: RecentItem[] = useMemo(
    () =>
      projects.slice(0, 3).map((p) => ({
        id: p.id,
        title: p.name,
        dur: p.video_path.split(/[/\\]/).pop() ?? "",
        statusLabel: p.status,
      })),
    [projects],
  );
  const dubItems: DubListItem[] = projects.map((p) => ({ id: p.id, name: p.name, video_path: p.video_path, status: p.status }));

  const projectTabs = tabs.filter((t) => t.kind === "project");
  const isOverlay = active.kind === "home" || active.kind === "dub" || active.kind === "project";

  if (gate === "checking") return null; // dark native window bg while probing
  if (gate === "onboard") {
    return <Onboarding onDone={() => setGate("app")} />;
  }

  return (
    <div className="bss" style={{ height: "100vh", width: "100vw", display: "flex", flexDirection: "column", background: C.appBg, color: "#fff", fontFamily: FONT, fontSize: 13, overflow: "hidden", WebkitFontSmoothing: "antialiased" }}>
      {/* tab strip (the OS provides the title bar + window controls) */}
      <div className="tabstrip" style={{ height: 40, flex: "none", display: "flex", alignItems: "flex-end", padding: "0 8px", gap: 3, background: C.titlebar, borderBottom: `1px solid ${C.borderSoft}`, overflowX: "auto" }}>
        {tabs.map((t) => {
          const on = t.id === activeId;
          const title = t.kind === "home" ? "Trang chủ" : t.kind === "project" ? (t.title ?? "Dự án") : FEATURE_TITLE[t.kind];
          const iconName = t.kind === "home" ? "home" : t.kind === "project" ? "film" : FEATURE_ICON[t.kind];
          return (
            <HoverBox
              key={t.id}
              onClick={() => activate(t)}
              title={title}
              style={{ height: 33, flex: "none", display: "flex", alignItems: "center", gap: 8, padding: "0 10px 0 12px", borderRadius: "9px 9px 0 0", cursor: "pointer", position: "relative", maxWidth: 200, background: on ? C.content : "transparent", border: `1px solid ${on ? C.borderSoft : "transparent"}`, borderBottom: "none", color: on ? "#fff" : C.muted }}
              hoverStyle={on ? undefined : { background: "#201e2a" }}
            >
              {on && <span style={{ position: "absolute", top: 0, left: 10, right: 10, height: 2, background: C.purple, borderRadius: "0 0 2px 2px" }} />}
              <span style={{ flex: "none", display: "grid", placeItems: "center", color: on ? C.purpleLt : C.muted3 }}>
                <Icon name={iconName} size={14} stroke={1.7} />
              </span>
              <span style={{ fontSize: 12.5, fontWeight: on ? 600 : 500, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{title}</span>
              {t.kind !== "home" && (
                <HoverBox
                  as="button"
                  onClick={closeTab(t.id)}
                  style={{ flex: "none", width: 18, height: 18, border: "none", background: "transparent", color: C.muted3, borderRadius: 5, display: "grid", placeItems: "center", cursor: "pointer", marginLeft: 1 }}
                  hoverStyle={{ background: "#393C49", color: "#fff" }}
                >
                  <Icon name="close" size={12} stroke={2.2} />
                </HoverBox>
              )}
            </HoverBox>
          );
        })}
        <HoverBox
          as="button"
          onClick={() => setActiveId("home")}
          title="Trang chủ"
          style={{ flex: "none", width: 30, height: 30, marginBottom: 1.5, border: "none", background: "transparent", color: C.muted, borderRadius: 7, display: "grid", placeItems: "center", cursor: "pointer" }}
          hoverStyle={{ background: "#252331", color: "#fff" }}
        >
          <Icon name="plus" size={17} stroke={2} />
        </HoverBox>
      </div>

      {/* content */}
      <div style={{ flex: 1, minHeight: 0, background: C.content, position: "relative" }}>
        {/* router-backed feature pages (TTS / Flows / Runs / Settings / API) */}
        <div style={{ position: "absolute", inset: 0, display: isOverlay ? "none" : "block", overflow: "auto", background: "#0d1117" }}>
          <div style={{ padding: 20, minHeight: "100%" }}>
            <Outlet />
          </div>
        </div>

        {/* home overlay */}
        <div style={{ position: "absolute", inset: 0, display: active.kind === "home" ? "block" : "none" }}>
          <Home onOpenFeature={openFeature} recents={recents} onOpenProject={openProject} />
        </div>

        {/* dubbing list overlay */}
        <div style={{ position: "absolute", inset: 0, display: active.kind === "dub" ? "block" : "none" }}>
          <DubList projects={dubItems} onOpen={openProject} onNew={newProject} />
        </div>

        {/* project editor tabs — kept mounted per tab */}
        {projectTabs.map((t) => (
          <div key={t.id} style={{ position: "absolute", inset: 0, display: t.id === activeId ? "block" : "none" }}>
            <VideoStudio projectId={t.projectId ?? ""} title={t.title} />
          </div>
        ))}
      </div>
    </div>
  );
}
