import { ClipboardList, Home, Library, Settings } from "lucide-react";
import type { Dispatch, ReactNode, SetStateAction } from "react";
import { asset } from "../shared/utils";

export type AppPage = "home" | "library" | "history" | "settings";

const navItems: Array<{ id: AppPage; label: string; icon: typeof Home }> = [
  { id: "home", label: "项目首页", icon: Home },
  { id: "library", label: "本地动漫", icon: Library },
  { id: "history", label: "整理记录", icon: ClipboardList },
  { id: "settings", label: "设置", icon: Settings },
];

export function AppShell(props: {
  activeNav: AppPage;
  setActiveNav: Dispatch<SetStateAction<AppPage>>;
  toast: string | null;
  taskDock?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <img src={asset("images/app_logo.png")} alt="" />
          <h1>Anime Subtitle Manager</h1>
          <p>mpv_tidy</p>
        </div>
        <nav className="side-nav">
          {navItems.map((item) => {
            const Icon = item.icon;
            return (
              <button
                className={props.activeNav === item.id ? "active" : ""}
                key={item.id}
                onClick={() => props.setActiveNav(item.id)}
                aria-current={props.activeNav === item.id ? "page" : undefined}
              >
                <Icon size={21} />
                <span>{item.label}</span>
              </button>
            );
          })}
        </nav>
        <div className="mascot">
          <img src={asset("images/sidebar_character.png")} alt="" />
        </div>
      </aside>

      {props.children}
      {props.taskDock}
      {props.toast && <div className="toast">{props.toast}</div>}
    </div>
  );
}
