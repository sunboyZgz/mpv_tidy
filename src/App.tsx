import { useState } from "react";
import { AppShell, type AppPage } from "./components/AppShell";
import { LocalAnimePage } from "./features/localAnime/LocalAnimePage";
import { ProjectHomePage, type OrganizeTaskUi } from "./features/projectHome/ProjectHomePage";
import { SettingsPage } from "./features/settings/SettingsPage";
import type { LocalAnimeLibraryEntry } from "./types";

function App() {
  const [activeNav, setActiveNav] = useState<AppPage>("home");
  const [toast, setToast] = useState<string | null>(null);
  const [savedLibraryEntry, setSavedLibraryEntry] = useState<LocalAnimeLibraryEntry | null>(null);
  const [organizeTasks, setOrganizeTasks] = useState<OrganizeTaskUi[]>([]);
  const [taskDockHidden, setTaskDockHidden] = useState(false);

  function showToast(message: string) {
    setToast(message);
    window.setTimeout(() => setToast(null), 2600);
  }

  return (
    <AppShell
      activeNav={activeNav}
      setActiveNav={setActiveNav}
      toast={toast}
      taskDock={
        <OrganizeTaskDock
          hidden={taskDockHidden}
          tasks={organizeTasks}
          onClearCompleted={() => setOrganizeTasks((current) => current.filter((task) => task.status === "running"))}
          onShowHome={() => setActiveNav("home")}
          setHidden={setTaskDockHidden}
        />
      }
    >
      <div className="page-slot" hidden={activeNav !== "home"}>
        <ProjectHomePage
          showToast={showToast}
          onLibraryEntrySaved={setSavedLibraryEntry}
          organizeTasks={organizeTasks}
          setOrganizeTasks={setOrganizeTasks}
        />
      </div>
      <div className="page-slot" hidden={activeNav !== "library"}>
        <LocalAnimePage showToast={showToast} syncedEntry={savedLibraryEntry} />
      </div>
      <div className="page-slot" hidden={activeNav !== "history"}>
        <PlaceholderPage title="整理记录" />
      </div>
      <div className="page-slot" hidden={activeNav !== "settings"}>
        <SettingsPage showToast={showToast} />
      </div>
    </AppShell>
  );
}

function OrganizeTaskDock(props: {
  hidden: boolean;
  tasks: OrganizeTaskUi[];
  setHidden: (hidden: boolean) => void;
  onShowHome: () => void;
  onClearCompleted: () => void;
}) {
  const runningCount = props.tasks.filter((task) => task.status === "running").length;
  const latestTask = props.tasks[0] ?? null;

  if (props.tasks.length === 0) {
    return null;
  }

  if (props.hidden) {
    return (
      <button className="task-dock-tab" onClick={() => props.setHidden(false)}>
        <span>{runningCount > 0 ? runningCount : props.tasks.length}</span>
        整理任务
      </button>
    );
  }

  return (
    <aside className="task-dock" aria-label="整理任务列表">
      <div className="task-dock-header">
        <div>
          <strong>整理任务</strong>
          <small>{runningCount > 0 ? `${runningCount} 个任务进行中` : "暂无运行中的任务"}</small>
        </div>
        <div>
          <button onClick={props.onClearCompleted}>清除完成</button>
          <button onClick={() => props.setHidden(true)}>隐藏</button>
        </div>
      </div>
      <div className="task-dock-list">
        {props.tasks.map((task) => {
          const percent = Math.round((task.processed / Math.max(task.total, 1)) * 100);
          return (
            <button className={`task-card ${task.status}`} key={task.id} onClick={props.onShowHome}>
              <div>
                <strong>{task.title}</strong>
                <span>{task.message}</span>
              </div>
              <em>{percent}%</em>
              <i>
                <b style={{ width: `${percent}%` }} />
              </i>
            </button>
          );
        })}
      </div>
      {latestTask?.currentDestination && <p title={latestTask.currentDestination}>{latestTask.currentDestination}</p>}
    </aside>
  );
}

function PlaceholderPage({ title }: { title: string }) {
  return (
    <main className="workspace placeholder-page">
      <section>
        <h1>{title}</h1>
        <p>这个页面会在后续阶段接入。</p>
      </section>
    </main>
  );
}

export default App;
