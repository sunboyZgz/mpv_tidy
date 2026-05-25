import { useState } from "react";
import { AppShell, type AppPage } from "./components/AppShell";
import { LocalAnimePage } from "./features/localAnime/LocalAnimePage";
import { ProjectHomePage } from "./features/projectHome/ProjectHomePage";
import { SettingsPage } from "./features/settings/SettingsPage";
import type { LocalAnimeLibraryEntry } from "./types";

function App() {
  const [activeNav, setActiveNav] = useState<AppPage>("home");
  const [toast, setToast] = useState<string | null>(null);
  const [savedLibraryEntry, setSavedLibraryEntry] = useState<LocalAnimeLibraryEntry | null>(null);

  function showToast(message: string) {
    setToast(message);
    window.setTimeout(() => setToast(null), 2600);
  }

  return (
    <AppShell activeNav={activeNav} setActiveNav={setActiveNav} toast={toast}>
      {activeNav === "home" && <ProjectHomePage showToast={showToast} onLibraryEntrySaved={setSavedLibraryEntry} />}
      {activeNav === "library" && <LocalAnimePage showToast={showToast} syncedEntry={savedLibraryEntry} />}
      {activeNav === "history" && <PlaceholderPage title="整理记录" />}
      {activeNav === "settings" && <SettingsPage showToast={showToast} />}
    </AppShell>
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
