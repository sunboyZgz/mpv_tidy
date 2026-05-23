import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Check,
  ChevronDown,
  ClipboardList,
  Copy,
  Edit3,
  FolderOpen,
  Home,
  Info,
  Library,
  Loader2,
  Minus,
  Play,
  Plus,
  RefreshCcw,
  Rocket,
  RotateCcw,
  Save,
  Search,
  Settings,
  X,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type {
  CollisionAction,
  EpisodeKey,
  EpisodeMatch,
  LanguageCode,
  LocalAnimeLibraryEntry,
  MatchStatus,
  OrganizeExecutionResult,
  OrganizeMode,
  OrganizePlan,
  SaveLocalLibraryRequest,
  ScanAndMatchResult,
  ScannedSubtitle,
  ScannedVideo,
  SubtitleCandidate,
} from "./types";

type AppPage = "home" | "library" | "history" | "settings";
type ScanState = "idle" | "scanning" | "ready";
type WatchStatus = "watched" | "partial" | "unwatched";
type SubtitleFormat = "ass" | "srt" | "ssa" | "vtt" | "unknown";

interface DrawerDraft {
  episodeKey: string;
  videoPath: string;
  primarySubtitlePath: string;
  secondarySubtitlePath: string;
  note: string;
}

interface LocalSubtitle {
  id: string;
  path: string;
  language: LanguageCode;
  format: SubtitleFormat;
  role?: "primary" | "secondary" | "candidate";
}

interface LocalEpisode {
  id: string;
  episodeKey: string;
  title: string;
  videoPath: string;
  durationSec?: number;
  resolution?: string;
  codec?: string;
  fileSizeBytes?: number;
  subtitles: LocalSubtitle[];
  watchStatus: WatchStatus;
  lastPositionSec?: number;
  progressPercent?: number;
}

interface LocalAnimeEntryUi {
  id: string;
  title: string;
  alias?: string[];
  year?: number;
  type?: string;
  tags?: string[];
  description?: string;
  coverTone: string;
  rootDir: string;
  videoDir: string;
  subtitleDirs: string[];
  subtitleLanguages: LanguageCode[];
  episodes: LocalEpisode[];
  lastWatchedEpisodeId?: string;
  createdAt: string;
  updatedAt: string;
  notes?: string;
}

interface DirectorySubtitleOffsetPreference {
  directoryKey: string;
  primarySubtitleOffsetSec: number;
  secondarySubtitleOffsetSec: number;
  rememberOffset: boolean;
  updatedAt: string;
}

interface AnimeEditDraft {
  title: string;
  aliasText: string;
  year: string;
  type: string;
  tagsText: string;
  description: string;
  coverTone: string;
  notes: string;
}

const navItems: Array<{ id: AppPage; label: string; icon: typeof Home }> = [
  { id: "home", label: "项目首页", icon: Home },
  { id: "library", label: "本地动漫", icon: Library },
  { id: "history", label: "整理记录", icon: ClipboardList },
  { id: "settings", label: "设置", icon: Settings },
];

const statusLabel: Record<MatchStatus, string> = {
  matched: "已匹配",
  pendingFix: "待修正",
  conflict: "冲突",
  unprocessed: "未处理",
  missingVideo: "缺失视频",
  missingSub: "未完整",
};

const watchStatusLabel: Record<WatchStatus, string> = {
  watched: "已观看",
  partial: "部分观看",
  unwatched: "未观看",
};

const languageLabels: Record<LanguageCode, string> = {
  "zh-Hans": "zh-Hans",
  "zh-Hant": "zh-Hant",
  ja: "ja",
  en: "en",
  und: "und",
};

const asset = (path: string) => `/assets/${path}`;
const browserPreviewMessage = "当前是浏览器预览；真实扫描、整理和保存请在 Tauri 桌面窗口中执行。";

function App() {
  const [activeNav, setActiveNav] = useState<AppPage>("home");
  const [toast, setToast] = useState<string | null>(null);

  function showToast(message: string) {
    setToast(message);
    window.setTimeout(() => setToast(null), 2600);
  }

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <img src={asset("images/app_logo.png")} alt="" />
          <h1>Anime Subtitle Manager</h1>
        </div>
        <nav className="side-nav">
          {navItems.map((item) => {
            const Icon = item.icon;
            return (
              <button
                className={activeNav === item.id ? "active" : ""}
                key={item.id}
                onClick={() => setActiveNav(item.id)}
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

      {activeNav === "home" && <ProjectHomePage showToast={showToast} />}
      {activeNav === "library" && <LocalAnimePage showToast={showToast} />}
      {activeNav === "history" && <PlaceholderPage title="整理记录" />}
      {activeNav === "settings" && <PlaceholderPage title="设置" />}
      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}

function ProjectHomePage({ showToast }: { showToast: (message: string) => void }) {
  const [projectName, setProjectName] = useState("Jujutsu Kaisen");
  const [season, setSeason] = useState("S01");
  const [videoDir, setVideoDir] = useState<string | null>(null);
  const [subtitleDirs, setSubtitleDirs] = useState<string[]>([]);
  const [outputDir, setOutputDir] = useState<string | null>(null);
  const [outputHistory, setOutputHistory] = useState<string[]>([]);
  const [scanState, setScanState] = useState<ScanState>("idle");
  const [scanResult, setScanResult] = useState<ScanAndMatchResult | null>(null);
  const [selectedEpisodeKey, setSelectedEpisodeKey] = useState<string | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [organizeMode, setOrganizeMode] = useState<OrganizeMode>("copy");
  const [plan, setPlan] = useState<OrganizePlan | null>(null);
  const [completedPlan, setCompletedPlan] = useState<OrganizePlan | null>(null);
  const [organizedResult, setOrganizedResult] = useState<OrganizeExecutionResult | null>(null);
  const [librarySaved, setLibrarySaved] = useState(false);
  const [message, setMessage] = useState("请选择视频目录、字幕目录和输出目录，然后开始扫描。");
  const [drawerDraft, setDrawerDraft] = useState<DrawerDraft>({
    episodeKey: "",
    videoPath: "",
    primarySubtitlePath: "",
    secondarySubtitlePath: "",
    note: "",
  });

  const matches = scanResult?.matches ?? [];
  const expectedSubtitleCount = Math.max(1, subtitleDirs.length);

  const selectedMatch = useMemo(() => {
    if (!selectedEpisodeKey) {
      return null;
    }
    return matches.find((item) => item.episodeKey === selectedEpisodeKey) ?? null;
  }, [matches, selectedEpisodeKey]);

  const stats = useMemo(() => {
    const effectiveStatuses = matches.map((item) => getEffectiveStatus(item, expectedSubtitleCount));
    return {
      videos: scanResult?.scan.videos.length ?? 0,
      subtitles: scanResult?.scan.subtitles.length ?? 0,
      matched: effectiveStatuses.filter((status) => status === "matched").length,
      incomplete: effectiveStatuses.filter((status) => status === "missingSub" || status === "missingVideo").length,
      conflict: effectiveStatuses.filter((status) => status === "conflict").length,
    };
  }, [expectedSubtitleCount, matches, scanResult]);

  useEffect(() => {
    if (!selectedMatch) {
      return;
    }
    setDrawerDraft({
      episodeKey: selectedMatch.episodeKey,
      videoPath: selectedMatch.video?.path ?? "",
      primarySubtitlePath: selectedMatch.primarySubtitle?.path ?? "",
      secondarySubtitlePath: selectedMatch.secondarySubtitle?.path ?? "",
      note: "",
    });
  }, [selectedMatch]);

  async function chooseVideoDir() {
    if (!isTauriRuntime()) {
      setVideoDir("D:\\Anime\\Jujutsu Kaisen\\videos");
      setMessage(browserPreviewMessage);
      return;
    }
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setVideoDir(selected);
      setScanResult(null);
      setScanState("idle");
      setSelectedEpisodeKey(null);
      setDrawerOpen(false);
    }
  }

  async function addSubtitleDirs() {
    if (!isTauriRuntime()) {
      setSubtitleDirs((current) =>
        unique([...current, "D:\\Anime\\Jujutsu Kaisen\\subs", "D:\\Anime\\Jujutsu Kaisen\\subs_2"]),
      );
      setMessage(browserPreviewMessage);
      return;
    }
    const selected = await open({ directory: true, multiple: true });
    if (!selected) {
      return;
    }
    const paths = Array.isArray(selected) ? selected : [selected];
    setSubtitleDirs((current) => unique([...current, ...paths]));
    setScanResult(null);
    setScanState("idle");
    setDrawerOpen(false);
  }

  async function chooseOutputDir() {
    if (!isTauriRuntime()) {
      const sample = "D:\\整理输出\\Jujutsu Kaisen S01";
      setOutputDir(sample);
      setOutputHistory((current) => unique([sample, ...current]).slice(0, 3));
      setMessage(browserPreviewMessage);
      return;
    }
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setOutputDir(selected);
      setOutputHistory((current) => unique([selected, ...current]).slice(0, 3));
    }
  }

  async function scanAndMatch() {
    if (!videoDir || subtitleDirs.length === 0) {
      setMessage("请先选择一个视频目录，并添加至少一个字幕目录。");
      return;
    }

    setScanState("scanning");
    setMessage("正在扫描视频和字幕文件...");
    setOrganizedResult(null);
    setCompletedPlan(null);
    setLibrarySaved(false);

    if (!isTauriRuntime()) {
      window.setTimeout(() => {
        const demo = makeDemoScanResult();
        setScanResult(demo);
        setSelectedEpisodeKey(demo.matches[2]?.episodeKey ?? demo.matches[0]?.episodeKey ?? null);
        setDrawerOpen(true);
        setScanState("ready");
        setMessage("浏览器预览已加载示例扫描结果。");
      }, 180);
      return;
    }

    try {
      const result = await invoke<ScanAndMatchResult>("scan_and_match", {
        input: { videoDirs: [videoDir], subtitleDirs },
      });
      setScanResult(result);
      setSelectedEpisodeKey(result.matches[0]?.episodeKey ?? null);
      setDrawerOpen(false);
      setScanState("ready");
      setMessage(`扫描完成：${result.scan.videos.length} 个视频，${result.scan.subtitles.length} 个字幕。`);
    } catch (error) {
      setScanState(scanResult ? "ready" : "idle");
      setMessage(String(error));
    }
  }

  async function startOrganize() {
    if (!outputDir) {
      setMessage("请先选择输出目录。");
      return;
    }
    if (matches.length === 0) {
      setMessage("请先完成扫描并确认匹配结果。");
      return;
    }
    if (!isTauriRuntime()) {
      setPlan(makeDemoPlan(projectName, season, outputDir, organizeMode, matches));
      setMessage("浏览器预览已生成示例整理计划。");
      return;
    }

    try {
      const result = await invoke<OrganizePlan>("build_organize_plan", {
        request: {
          projectName,
          season,
          outputDir,
          matches,
          mode: organizeMode,
          primaryLanguage: "zh-Hans",
          secondaryLanguage: "ja",
        },
      });
      setPlan(result);
      setMessage(result.hasConflicts ? "整理计划已生成，存在冲突项，请先确认处理方式。" : "整理计划已生成，请确认执行。");
    } catch (error) {
      setMessage(String(error));
    }
  }

  async function executePlan() {
    if (!plan) {
      return;
    }

    if (!isTauriRuntime()) {
      setOrganizedResult({
        items: plan.items,
        mapWritten: true,
        message: plan.mode === "copy" ? "示例复制整理完成。" : "示例移动整理完成。",
      });
      setCompletedPlan(plan);
      setPlan(null);
      setLibrarySaved(false);
      setMessage("浏览器预览已模拟整理完成。");
      return;
    }

    try {
      const result = await invoke<OrganizeExecutionResult>("execute_organize_plan", { plan });
      setOrganizedResult(result);
      setCompletedPlan(plan);
      setPlan(null);
      setLibrarySaved(false);
      setOutputHistory((current) => unique([plan.outputDir, ...current]).slice(0, 3));
      setMessage(result.message);
    } catch (error) {
      setMessage(String(error));
    }
  }

  async function saveToLocalAnime() {
    if (!organizedResult || !completedPlan) {
      setMessage("请先完成整理，再保存到本地动漫。");
      return;
    }

    const request: SaveLocalLibraryRequest = {
      projectName,
      season,
      outputDir: completedPlan.outputDir,
      mode: completedPlan.mode,
      episodes: matches.map((item) => ({
        episodeKey: item.episodeKey,
        videoPath: item.video?.path ?? null,
        primarySubtitlePath: item.primarySubtitle?.path ?? null,
        secondarySubtitlePath: item.secondarySubtitle?.path ?? null,
        subtitleCount: item.candidates.length,
        status: getEffectiveStatus(item, expectedSubtitleCount),
      })),
    };

    if (!isTauriRuntime()) {
      setLibrarySaved(true);
      setMessage("浏览器预览已模拟保存到本地动漫。");
      showToast("已保存到本地动漫");
      return;
    }

    try {
      const saved = await invoke<LocalAnimeLibraryEntry>("save_local_library_entry", { request });
      setLibrarySaved(true);
      setMessage(`已保存到本地动漫：${saved.projectName} ${saved.season}`);
      showToast("已保存到本地动漫");
    } catch (error) {
      setMessage(String(error));
    }
  }

  function selectMode(mode: OrganizeMode) {
    setOrganizeMode(mode);
    setMessage(mode === "copy" ? "已选择复制整理模式，原文件将保留。" : "已选择移动整理模式，执行前会显示确认计划。");
  }

  function openDrawer(item: EpisodeMatch) {
    setSelectedEpisodeKey(item.episodeKey);
    setDrawerOpen(true);
  }

  function applyManualCorrection() {
    const parsedEpisode = parseEpisodeKey(drawerDraft.episodeKey);
    if (!selectedMatch || !parsedEpisode) {
      setMessage("请输入类似 S01E03 的有效集数键。");
      return;
    }

    setScanResult((current) => {
      if (!current) {
        return current;
      }
      const nextKey = formatEpisodeKey(parsedEpisode);
      const nextMatches = current.matches.map((item) => {
        if (item.episodeKey !== selectedMatch.episodeKey) {
          return item;
        }
        const video = current.scan.videos.find((candidate) => candidate.path === drawerDraft.videoPath) ?? item.video;
        const primarySubtitle =
          item.candidates.find((candidate) => candidate.path === drawerDraft.primarySubtitlePath) ?? item.primarySubtitle;
        const secondarySubtitle =
          item.candidates.find((candidate) => candidate.path === drawerDraft.secondarySubtitlePath) ?? item.secondarySubtitle;
        return {
          ...item,
          episode: parsedEpisode,
          episodeKey: nextKey,
          video,
          primarySubtitle,
          secondarySubtitle,
          notes: drawerDraft.note ? unique([...item.notes, drawerDraft.note]) : item.notes,
        };
      });
      return { ...current, matches: nextMatches };
    });
    setSelectedEpisodeKey(formatEpisodeKey(parsedEpisode));
    setMessage("已应用手动修正到当前扫描结果。");
  }

  function updateCollision(index: number, action: CollisionAction) {
    setPlan((current) => {
      if (!current) {
        return current;
      }
      return {
        ...current,
        items: current.items.map((item, itemIndex) =>
          itemIndex === index ? { ...item, collisionAction: action } : item,
        ),
      };
    });
  }

  return (
    <main className="workspace">
      <header className="topbar">
        <div className="project-title">
          <input aria-label="项目名称" value={projectName} onChange={(event) => setProjectName(event.target.value)} />
          <input
            aria-label="季"
            className="season-input"
            value={season}
            onChange={(event) => setSeason(event.target.value.toUpperCase())}
          />
          <span className={`scan-badge ${scanResult ? "done" : ""}`}>
            <Check size={15} />
            {scanResult ? "已扫描" : "未扫描"}
          </span>
        </div>
        <WindowControls />
      </header>

      <section className="stage">
        <div className="main-column">
          <div className="resource-row">
            <DirectoryCard
              icon="status_folder.svg"
              title="视频目录"
              primaryText={videoDir ?? "尚未选择视频目录"}
              metaText="当前仅允许一个视频目录"
              buttonText="更换目录"
              onClick={chooseVideoDir}
            />
            <SubtitleDirectoryCard dirs={subtitleDirs} onAdd={addSubtitleDirs} />
            <DirectoryCard
              icon="status_folder.svg"
              title="输出目录"
              primaryText={outputDir ?? "尚未选择输出目录"}
              metaText={outputHistory[0] ? `最近：${outputHistory[0]}` : "最近整理：暂无记录"}
              buttonText="更换目录"
              accent
              onClick={chooseOutputDir}
            />
          </div>

          <div className="table-hint">
            <Info size={17} />
            <span>点击表格行或编辑按钮查看右侧详情</span>
          </div>

          <div className="stats-row">
            <StatCard icon="status_subtitle_file.svg" label="视频文件" value={stats.videos} />
            <StatCard icon="status_subtitle_file.svg" label="字幕文件" value={stats.subtitles} />
            <StatCard icon="status_checkmark.svg" label="已匹配集数" value={stats.matched} tone="success" />
            <StatCard icon="status_help_question.svg" label="未完整匹配" value={stats.incomplete} tone="warning" />
            <StatCard icon="status_warning_alert.svg" label="冲突项" value={stats.conflict} tone="danger" />
          </div>

          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th></th>
                  <th>集数</th>
                  <th>视频文件</th>
                  <th>字幕候选</th>
                  <th>字幕数量</th>
                  <th>状态</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                {matches.length === 0 ? (
                  <tr>
                    <td colSpan={7} className="empty">
                      扫描后将在这里显示视频与字幕候选。
                    </td>
                  </tr>
                ) : (
                  matches.map((item) => {
                    const status = getEffectiveStatus(item, expectedSubtitleCount);
                    return (
                      <tr
                        className={item.episodeKey === selectedEpisodeKey ? "selected" : ""}
                        key={item.episodeKey}
                        onClick={() => openDrawer(item)}
                      >
                        <td className="star">☆</td>
                        <td>{item.episodeKey}</td>
                        <td title={item.video?.path ?? ""}>{item.video?.fileName ?? "缺失视频"}</td>
                        <td>
                          <CandidateChips item={item} />
                        </td>
                        <td>{item.candidates.length}</td>
                        <td>
                          <StatusChip status={status} />
                        </td>
                        <td>
                          <button
                            className="edit-button"
                            onClick={(event) => {
                              event.stopPropagation();
                              openDrawer(item);
                            }}
                          >
                            <Edit3 size={15} />
                          </button>
                        </td>
                      </tr>
                    );
                  })
                )}
              </tbody>
            </table>
          </div>

          <div className="operation-row">
            <button className="op-button scan" disabled={scanState === "scanning"} onClick={scanAndMatch}>
              {scanState === "scanning" ? <Loader2 className="spin" size={20} /> : <RefreshCcw size={20} />}
              {scanState === "scanning" ? "扫描中..." : scanResult ? "重新扫描" : "开始扫描"}
            </button>
            <button
              className={`op-button ${organizeMode === "copy" ? "selected-mode" : ""}`}
              onClick={() => selectMode("copy")}
            >
              <Copy size={20} />
              复制整理
            </button>
            <button
              className={`op-button ${organizeMode === "move" ? "selected-mode" : ""}`}
              onClick={() => selectMode("move")}
            >
              <FolderOpen size={20} />
              移动整理
            </button>
            <button className="op-button organize" onClick={startOrganize}>
              <Rocket size={20} />
              开始整理
            </button>
            <button
              className="op-button save-library"
              disabled={!organizedResult || !completedPlan || librarySaved}
              onClick={saveToLocalAnime}
            >
              <Save size={20} />
              {librarySaved ? "已保存到本地动漫" : "保存到本地动漫"}
            </button>
          </div>

          <div className="output-preview">
            <div>
              <strong>输出结构预览（目标目录）</strong>
              <pre>{outputPreview(projectName, season)}</pre>
            </div>
            <img src={asset("images/background_lineart.png")} alt="" />
          </div>
        </div>

        <DetailDrawer
          allVideos={scanResult?.scan.videos ?? []}
          item={selectedMatch}
          draft={drawerDraft}
          isOpen={drawerOpen}
          setDraft={setDrawerDraft}
          onApply={applyManualCorrection}
          onClose={() => setDrawerOpen(false)}
        />
      </section>

      <div className="statusbar">{message}</div>

      {plan && (
        <PlanModal plan={plan} setPlan={setPlan} updateCollision={updateCollision} executePlan={executePlan} />
      )}
    </main>
  );
}

function LocalAnimePage({ showToast }: { showToast: (message: string) => void }) {
  const [library, setLibrary] = useState<LocalAnimeEntryUi[]>(() => makeDemoAnimeLibrary());
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedAnimeId, setSelectedAnimeId] = useState(() => selectDefaultAnimeId(makeDemoAnimeLibrary()));
  const [selectedEpisodeId, setSelectedEpisodeId] = useState<string | undefined>();
  const [primarySubtitleId, setPrimarySubtitleId] = useState<string | undefined>();
  const [secondarySubtitleId, setSecondarySubtitleId] = useState<string | undefined>();
  const [primaryOffset, setPrimaryOffset] = useState(0);
  const [secondaryOffset, setSecondaryOffset] = useState(0);
  const [rememberOffset, setRememberOffset] = useState(true);
  const [offsetPrefs, setOffsetPrefs] = useState<Record<string, DirectorySubtitleOffsetPreference>>({});
  const [isLaunching, setIsLaunching] = useState(false);
  const [panelMessage, setPanelMessage] = useState("选择剧集后可调整字幕并启动 MPV。");
  const [editingAnime, setEditingAnime] = useState<LocalAnimeEntryUi | null>(null);

  const filteredLibrary = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) {
      return library;
    }
    return library.filter((entry) =>
      [
        entry.title,
        ...(entry.alias ?? []),
        ...(entry.tags ?? []),
        entry.rootDir,
        entry.videoDir,
        ...entry.subtitleDirs,
      ].some((value) => value.toLowerCase().includes(query)),
    );
  }, [library, searchQuery]);

  const selectedAnime = useMemo(
    () => library.find((entry) => entry.id === selectedAnimeId) ?? filteredLibrary[0] ?? null,
    [filteredLibrary, library, selectedAnimeId],
  );

  const selectedEpisode = useMemo(() => {
    if (!selectedAnime) {
      return null;
    }
    return selectedAnime.episodes.find((episode) => episode.id === selectedEpisodeId) ?? selectedAnime.episodes[0] ?? null;
  }, [selectedAnime, selectedEpisodeId]);

  const selectedPrimarySubtitle = selectedEpisode?.subtitles.find((subtitle) => subtitle.id === primarySubtitleId) ?? null;
  const selectedSecondarySubtitle =
    selectedEpisode?.subtitles.find((subtitle) => subtitle.id === secondarySubtitleId) ?? null;
  const duplicateSubtitleSelected =
    !!selectedPrimarySubtitle && !!selectedSecondarySubtitle && selectedPrimarySubtitle.id === selectedSecondarySubtitle.id;

  const summary = useMemo(
    () => ({
      animeCount: library.length,
      episodeCount: library.reduce((sum, entry) => sum + entry.episodes.length, 0),
    }),
    [library],
  );

  useEffect(() => {
    if (!selectedAnime) {
      return;
    }
    const lastEpisode =
      selectedAnime.episodes.find((episode) => episode.id === selectedAnime.lastWatchedEpisodeId) ??
      selectedAnime.episodes[0];
    setSelectedEpisodeId(lastEpisode?.id);

    const directoryKey = normalizeDirectoryKey(selectedAnime.rootDir);
    const pref = offsetPrefs[directoryKey];
    if (pref) {
      setPrimaryOffset(pref.primarySubtitleOffsetSec);
      setSecondaryOffset(pref.secondarySubtitleOffsetSec);
      setRememberOffset(pref.rememberOffset);
      setPanelMessage("已加载当前目录的字幕偏移。");
    } else {
      setPrimaryOffset(0);
      setSecondaryOffset(0);
      setRememberOffset(true);
    }
  }, [selectedAnime?.id]);

  useEffect(() => {
    if (!selectedEpisode) {
      setPrimarySubtitleId(undefined);
      setSecondarySubtitleId(undefined);
      return;
    }
    const primary =
      selectedEpisode.subtitles.find((subtitle) => subtitle.role === "primary") ?? selectedEpisode.subtitles[0];
    const secondary =
      selectedEpisode.subtitles.find((subtitle) => subtitle.role === "secondary") ??
      selectedEpisode.subtitles.find((subtitle) => subtitle.id !== primary?.id);
    setPrimarySubtitleId(primary?.id);
    setSecondarySubtitleId(secondary?.id);
  }, [selectedEpisode]);

  function refreshLibrary() {
    setLibrary(makeDemoAnimeLibrary());
    showToast("本地动漫库已刷新");
  }

  function saveOffsetPreference(nextPrimary = primaryOffset, nextSecondary = secondaryOffset, nextRemember = rememberOffset) {
    if (!selectedAnime || !nextRemember) {
      return;
    }
    const directoryKey = normalizeDirectoryKey(selectedAnime.rootDir);
    setOffsetPrefs((current) => ({
      ...current,
      [directoryKey]: {
        directoryKey,
        primarySubtitleOffsetSec: nextPrimary,
        secondarySubtitleOffsetSec: nextSecondary,
        rememberOffset: nextRemember,
        updatedAt: new Date().toISOString(),
      },
    }));
    showToast("当前目录字幕偏移已保存");
  }

  async function playWithMpv() {
    if (!selectedEpisode) {
      setPanelMessage("请先选择一个剧集。");
      return;
    }
    if (duplicateSubtitleSelected) {
      setPanelMessage("主字幕和副字幕不能选择同一个文件。");
      return;
    }

    setIsLaunching(true);
    const extraArgs: string[] = [];
    if (primaryOffset !== 0) {
      extraArgs.push(`--sub-delay=${primaryOffset.toFixed(1)}`);
    }
    if (secondaryOffset !== 0) {
      setPanelMessage("副字幕偏移已保存；当前安全 MPV 参数策略仅应用主字幕偏移。");
    }

    if (!isTauriRuntime()) {
      window.setTimeout(() => {
        setIsLaunching(false);
        markEpisodeWatched(selectedEpisode.id, "partial");
        showToast("MPV 已启动");
      }, 320);
      return;
    }

    try {
      await invoke("launch_mpv", {
        request: {
          mpvPath: "mpv",
          videoPath: selectedEpisode.videoPath,
          primarySubtitle: selectedPrimarySubtitle?.path ?? null,
          secondarySubtitle: selectedSecondarySubtitle?.path ?? null,
          extraArgs,
        },
      });
      markEpisodeWatched(selectedEpisode.id, "partial");
      showToast("MPV 已启动");
    } catch (error) {
      setPanelMessage(String(error));
    } finally {
      setIsLaunching(false);
    }
  }

  function markEpisodeWatched(episodeId: string, status: WatchStatus) {
    if (!selectedAnime) {
      return;
    }
    setLibrary((current) =>
      current.map((entry) =>
        entry.id === selectedAnime.id
          ? {
              ...entry,
              lastWatchedEpisodeId: episodeId,
              episodes: entry.episodes.map((episode) =>
                episode.id === episodeId
                  ? { ...episode, watchStatus: status, progressPercent: Math.max(episode.progressPercent ?? 0, 12) }
                  : episode,
              ),
              updatedAt: new Date().toISOString(),
            }
          : entry,
      ),
    );
  }

  async function revealPath(path: string | undefined, label: string) {
    if (!path) {
      setPanelMessage(`${label}路径不存在。`);
      return;
    }
    if (!isTauriRuntime()) {
      showToast(`浏览器预览：打开${label}位置`);
      return;
    }
    try {
      await invoke("reveal_path", { path });
    } catch (error) {
      setPanelMessage(String(error));
    }
  }

  function saveAnimeInfo(draft: AnimeEditDraft) {
    if (!editingAnime) {
      return;
    }
    setLibrary((current) =>
      current.map((entry) =>
        entry.id === editingAnime.id
          ? {
              ...entry,
              title: draft.title.trim() || entry.title,
              alias: splitTextList(draft.aliasText),
              year: Number(draft.year) || undefined,
              type: draft.type.trim() || undefined,
              tags: splitTextList(draft.tagsText),
              description: draft.description.trim(),
              coverTone: draft.coverTone.trim() || entry.coverTone,
              notes: draft.notes.trim(),
              updatedAt: new Date().toISOString(),
            }
          : entry,
      ),
    );
    setEditingAnime(null);
    showToast("信息已保存");
  }

  return (
    <main className="workspace library-workspace">
      <header className="library-header">
        <h1>本地动漫</h1>
        <label className="library-search">
          <Search size={19} />
          <input
            value={searchQuery}
            onChange={(event) => setSearchQuery(event.target.value)}
            placeholder="搜索本地标题 / 别名 / 标签..."
          />
          {searchQuery && (
            <button onClick={() => setSearchQuery("")} aria-label="清空搜索">
              <X size={16} />
            </button>
          )}
        </label>
        <WindowControls />
      </header>

      <section className="library-stage">
        <div className="library-main">
          <div className="library-summary-row">
            <SummaryCard icon={<Play size={28} />} label="本地动漫" value={summary.animeCount} suffix="个系列" />
            <SummaryCard icon={<Play size={28} />} label="总集数" value={summary.episodeCount} suffix="集" />
          </div>

          <div className="library-content-grid">
            <aside className="anime-list-panel">
              <div className="panel-heading">
                <h2>我的动漫库（{filteredLibrary.length}）</h2>
                <button onClick={refreshLibrary} aria-label="刷新本地动漫库">
                  <RefreshCcw size={17} />
                </button>
              </div>
              <div className="anime-list-scroll">
                {filteredLibrary.length === 0 ? (
                  <EmptyState
                    title={library.length === 0 ? "暂无本地动漫" : "没有找到匹配的本地动漫"}
                    body={
                      library.length === 0
                        ? "请先在项目首页完成整理并保存到本地动漫"
                        : "换一个标题、别名或标签试试看"
                    }
                  />
                ) : (
                  filteredLibrary.map((entry) => (
                    <AnimeListItem
                      entry={entry}
                      active={entry.id === selectedAnime?.id}
                      key={entry.id}
                      onClick={() => setSelectedAnimeId(entry.id)}
                    />
                  ))
                )}
              </div>
            </aside>

            <section className="anime-center-column">
              <AnimeDetailCard entry={selectedAnime} onEdit={() => selectedAnime && setEditingAnime(selectedAnime)} />
              <EpisodeTable
                anime={selectedAnime}
                selectedEpisodeId={selectedEpisode?.id}
                onSelect={(episode) => setSelectedEpisodeId(episode.id)}
                onDoubleClick={() => {
                  void playWithMpv();
                }}
              />
            </section>
          </div>
        </div>

        <PlaybackPanel
          anime={selectedAnime}
          episode={selectedEpisode}
          primarySubtitleId={primarySubtitleId}
          secondarySubtitleId={secondarySubtitleId}
          setPrimarySubtitleId={setPrimarySubtitleId}
          setSecondarySubtitleId={setSecondarySubtitleId}
          primaryOffset={primaryOffset}
          secondaryOffset={secondaryOffset}
          setPrimaryOffset={(value) => {
            setPrimaryOffset(value);
            saveOffsetPreference(value, secondaryOffset, rememberOffset);
          }}
          setSecondaryOffset={(value) => {
            setSecondaryOffset(value);
            saveOffsetPreference(primaryOffset, value, rememberOffset);
          }}
          rememberOffset={rememberOffset}
          setRememberOffset={(value) => {
            setRememberOffset(value);
            saveOffsetPreference(primaryOffset, secondaryOffset, value);
          }}
          onResetOffsets={() => {
            setPrimaryOffset(0);
            setSecondaryOffset(0);
            saveOffsetPreference(0, 0, rememberOffset);
          }}
          duplicateSubtitleSelected={duplicateSubtitleSelected}
          isLaunching={isLaunching}
          message={panelMessage}
          onPlay={playWithMpv}
          onRevealVideo={() => revealPath(selectedEpisode?.videoPath, "视频")}
          onRevealSubtitle={() => revealPath(selectedPrimarySubtitle?.path, "字幕")}
        />
      </section>

      {editingAnime && (
        <AnimeEditModal anime={editingAnime} onCancel={() => setEditingAnime(null)} onSave={saveAnimeInfo} />
      )}
    </main>
  );
}

function PlaceholderPage({ title }: { title: string }) {
  return (
    <main className="workspace placeholder-page">
      <WindowControls />
      <section>
        <h1>{title}</h1>
        <p>这个页面会在后续阶段接入。</p>
      </section>
    </main>
  );
}

function WindowControls() {
  return (
    <div className="window-controls" aria-hidden="true">
      <span>−</span>
      <span>□</span>
      <span>×</span>
    </div>
  );
}

function DirectoryCard(props: {
  icon: string;
  title: string;
  primaryText: string;
  metaText: string;
  buttonText: string;
  accent?: boolean;
  onClick: () => void;
}) {
  return (
    <div className="resource-card">
      <img src={asset(`icons/${props.icon}`)} alt="" />
      <div>
        <h3>{props.title}</h3>
        <p title={props.primaryText}>{props.primaryText}</p>
        <small title={props.metaText}>{props.metaText}</small>
        <button className={props.accent ? "pink-outline" : "violet-outline"} onClick={props.onClick}>
          {props.buttonText}
        </button>
      </div>
    </div>
  );
}

function SubtitleDirectoryCard({ dirs, onAdd }: { dirs: string[]; onAdd: () => void }) {
  return (
    <div className="resource-card subtitle-card">
      <img src={asset("icons/status_subtitle_file.svg")} alt="" />
      <div>
        <h3>字幕目录（{dirs.length} 个）</h3>
        <div className="dir-pill-list">
          {dirs.length === 0 ? (
            <span className="dir-empty">尚未添加字幕目录</span>
          ) : (
            dirs.slice(0, 3).map((dir) => (
              <span className="dir-pill" title={dir} key={dir}>
                {dir}
              </span>
            ))
          )}
        </div>
        <button className="violet-outline" onClick={onAdd}>
          添加字幕目录
        </button>
      </div>
    </div>
  );
}

function StatCard(props: { icon: string; label: string; value: number; tone?: "success" | "warning" | "danger" }) {
  return (
    <div className={`stat-card ${props.tone ?? ""}`}>
      <img src={asset(`icons/${props.icon}`)} alt="" />
      <div>
        <span>{props.label}</span>
        <strong>{props.value}</strong>
      </div>
    </div>
  );
}

function SummaryCard(props: { icon: React.ReactNode; label: string; value: number; suffix: string }) {
  return (
    <div className="library-summary-card">
      <div className="summary-icon">{props.icon}</div>
      <div>
        <span>{props.label}</span>
        <strong>
          {props.value}
          <small>{props.suffix}</small>
        </strong>
      </div>
    </div>
  );
}

function AnimeListItem(props: { entry: LocalAnimeEntryUi; active: boolean; onClick: () => void }) {
  const progress = animeProgress(props.entry);
  const lastEpisode = props.entry.episodes.find((episode) => episode.id === props.entry.lastWatchedEpisodeId);
  return (
    <button className={`anime-list-item ${props.active ? "active" : ""}`} onClick={props.onClick}>
      <CoverBlock entry={props.entry} compact />
      <div className="anime-list-text">
        <strong>{props.entry.title}</strong>
        <span>{props.entry.episodes.length} 集</span>
        <span>上次观看：{lastEpisode?.episodeKey ?? "未开始"}</span>
        <div className="progress-line">
          <i style={{ width: `${progress}%` }} />
        </div>
      </div>
      <em>{progress}%</em>
    </button>
  );
}

function AnimeDetailCard({ entry, onEdit }: { entry: LocalAnimeEntryUi | null; onEdit: () => void }) {
  if (!entry) {
    return <EmptyState title="暂无本地动漫" body="请先在项目首页完成整理并保存到本地动漫" />;
  }
  return (
    <section className="anime-detail-card">
      <CoverBlock entry={entry} />
      <div className="anime-detail-body">
        <h2>{entry.title}</h2>
        <InfoGrid label="别名" value={entry.alias?.join(" / ")} />
        <InfoGrid label="年份" value={entry.year ? String(entry.year) : undefined} />
        <InfoGrid label="类型" value={entry.type} />
        {entry.tags && entry.tags.length > 0 && (
          <div className="tag-row">
            {entry.tags.map((tag) => (
              <span key={tag}>{tag}</span>
            ))}
          </div>
        )}
        <p>{entry.description || "暂无简介，可点击编辑信息补充。"}</p>
        <button className="edit-info-button" onClick={onEdit}>
          <Edit3 size={17} />
          编辑信息
        </button>
      </div>
    </section>
  );
}

function EpisodeTable(props: {
  anime: LocalAnimeEntryUi | null;
  selectedEpisodeId?: string;
  onSelect: (episode: LocalEpisode) => void;
  onDoubleClick: (episode: LocalEpisode) => void;
}) {
  return (
    <section className="episode-panel">
      <div className="episode-heading">
        <h2>剧集列表（{props.anime?.episodes.length ?? 0}）</h2>
        <div>
          <WatchLegend status="watched" />
          <WatchLegend status="partial" />
          <WatchLegend status="unwatched" />
        </div>
      </div>
      <div className="episode-table-wrap">
        <table className="episode-table">
          <thead>
            <tr>
              <th>集数</th>
              <th>标题</th>
              <th>状态</th>
              <th>字幕</th>
              <th>时长</th>
            </tr>
          </thead>
          <tbody>
            {!props.anime || props.anime.episodes.length === 0 ? (
              <tr>
                <td colSpan={5} className="empty">
                  暂无剧集
                </td>
              </tr>
            ) : (
              props.anime.episodes.map((episode) => (
                <tr
                  className={episode.id === props.selectedEpisodeId ? "selected" : ""}
                  key={episode.id}
                  onClick={() => props.onSelect(episode)}
                  onDoubleClick={() => props.onDoubleClick(episode)}
                >
                  <td>
                    <span className="episode-play-dot">＋</span>
                    {episode.episodeKey}
                  </td>
                  <td>{episode.title}</td>
                  <td>
                    <WatchStatusBadge status={episode.watchStatus} />
                  </td>
                  <td>
                    <div className="candidate-chip-row">
                      {unique(episode.subtitles.map((subtitle) => subtitle.language)).map((language) => (
                        <span className={`mini-chip ${language.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`} key={language}>
                          {language}
                        </span>
                      ))}
                    </div>
                  </td>
                  <td>{formatDuration(episode.durationSec)}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
      {props.anime && (
        <div className="episode-footer">
          <span>{props.anime.episodes.length} 集（共 1 季）</span>
          <span>已观看 {props.anime.episodes.filter((episode) => episode.watchStatus === "watched").length} 集</span>
          <span>总时长 {formatDuration(props.anime.episodes.reduce((sum, episode) => sum + (episode.durationSec ?? 0), 0))}</span>
        </div>
      )}
    </section>
  );
}

function PlaybackPanel(props: {
  anime: LocalAnimeEntryUi | null;
  episode: LocalEpisode | null;
  primarySubtitleId?: string;
  secondarySubtitleId?: string;
  setPrimarySubtitleId: (id: string | undefined) => void;
  setSecondarySubtitleId: (id: string | undefined) => void;
  primaryOffset: number;
  secondaryOffset: number;
  setPrimaryOffset: (value: number) => void;
  setSecondaryOffset: (value: number) => void;
  rememberOffset: boolean;
  setRememberOffset: (value: boolean) => void;
  onResetOffsets: () => void;
  duplicateSubtitleSelected: boolean;
  isLaunching: boolean;
  message: string;
  onPlay: () => void;
  onRevealVideo: () => void;
  onRevealSubtitle: () => void;
}) {
  const subtitles = props.episode?.subtitles ?? [];
  return (
    <aside className="playback-panel">
      <div className="playback-title">
        <button className="round-icon" aria-label="返回">
          <ChevronDown size={16} className="rotate-90" />
        </button>
        <h2>{props.episode?.episodeKey ?? "--"} {props.episode?.title ?? "未选择剧集"}</h2>
        <div>
          <span>‹</span>
          <span>›</span>
          <span>×</span>
        </div>
      </div>

      <PanelSection title="文件信息" icon="header_video_info.svg">
        <InfoRow label="文件名" value={fileNameFromPath(props.episode?.videoPath) || "未知"} />
        <InfoRow label="时长" value={formatDuration(props.episode?.durationSec)} />
        <InfoRow label="分辨率" value={props.episode?.resolution ?? "未知"} />
        <InfoRow label="编码" value={props.episode?.codec ?? "未知"} />
        <InfoRow label="文件大小" value={formatBytes(props.episode?.fileSizeBytes ?? 0)} />
        <InfoRow label="上次播放" value={formatPlaybackProgress(props.episode)} />
      </PanelSection>

      <PanelSection title="字幕选择">
        <SubtitleSelect
          label="主字幕"
          value={props.primarySubtitleId}
          subtitles={subtitles}
          onChange={props.setPrimarySubtitleId}
        />
        <SubtitleSelect
          label="副字幕"
          value={props.secondarySubtitleId}
          subtitles={subtitles}
          onChange={props.setSecondarySubtitleId}
        />
        <div className="available-subtitles">
          {unique(subtitles.flatMap((subtitle) => [subtitle.language, subtitle.format])).map((tag) => (
            <span className={`mini-chip ${String(tag).toLowerCase().replace(/[^a-z0-9]+/g, "-")}`} key={tag}>
              {tag}
            </span>
          ))}
        </div>
        {props.duplicateSubtitleSelected && <p className="inline-warning">主字幕和副字幕不能选择同一个文件。</p>}
      </PanelSection>

      <PanelSection title="字幕偏移（秒）">
        <OffsetControl label="主字幕偏移" value={props.primaryOffset} onChange={props.setPrimaryOffset} />
        <OffsetControl label="副字幕偏移" value={props.secondaryOffset} onChange={props.setSecondaryOffset} />
        <button className="reset-offset" onClick={props.onResetOffsets}>
          <RotateCcw size={15} />
          重置偏移
        </button>
        <label className="remember-offset">
          <input
            type="checkbox"
            checked={props.rememberOffset}
            onChange={(event) => props.setRememberOffset(event.target.checked)}
          />
          <span>
            记住当前目录的字幕偏移
            <small>仅对当前目录生效，偏移值互不影响</small>
          </span>
        </label>
      </PanelSection>

      <button
        className="mpv-play-button"
        disabled={!props.episode || props.isLaunching || props.duplicateSubtitleSelected}
        onClick={props.onPlay}
      >
        <Play size={22} />
        {props.isLaunching ? "正在启动 MPV..." : "用 MPV 播放"}
      </button>
      <div className="secondary-play-actions">
        <button onClick={props.onRevealVideo}>
          <FolderOpen size={17} />
          打开视频位置
        </button>
        <button onClick={props.onRevealSubtitle}>
          <FolderOpen size={17} />
          打开字幕位置
        </button>
      </div>
      <div className="playback-art">
        <img src={asset("images/background_lineart.png")} alt="" />
      </div>
      <p className="panel-message">{props.message}</p>
    </aside>
  );
}

function PanelSection(props: { title: string; icon?: string; children: React.ReactNode }) {
  return (
    <section className="playback-section">
      <h3>
        {props.icon && <img src={asset(`icons/${props.icon}`)} alt="" />}
        {props.title}
      </h3>
      {props.children}
    </section>
  );
}

function SubtitleSelect(props: {
  label: string;
  value?: string;
  subtitles: LocalSubtitle[];
  onChange: (id: string | undefined) => void;
}) {
  return (
    <label className="subtitle-select-row">
      <span>{props.label}</span>
      <select
        value={props.value ?? ""}
        disabled={props.subtitles.length === 0}
        onChange={(event) => props.onChange(event.target.value || undefined)}
      >
        {props.subtitles.length === 0 ? (
          <option value="">无可用字幕</option>
        ) : (
          <>
            <option value="">不使用</option>
            {props.subtitles.map((subtitle) => (
              <option value={subtitle.id} key={subtitle.id}>
                {subtitle.language}（{subtitle.format.toUpperCase()}）
              </option>
            ))}
          </>
        )}
      </select>
    </label>
  );
}

function OffsetControl(props: { label: string; value: number; onChange: (value: number) => void }) {
  const [draft, setDraft] = useState(props.value.toFixed(1));
  const [invalid, setInvalid] = useState(false);

  useEffect(() => {
    setDraft(props.value.toFixed(1));
    setInvalid(false);
  }, [props.value]);

  function commit(value: string) {
    if (!value.trim()) {
      setInvalid(true);
      return;
    }
    const numeric = Number(value);
    if (!Number.isFinite(numeric)) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    props.onChange(roundOffset(numeric));
  }

  return (
    <div className={`offset-control ${invalid ? "invalid" : ""}`}>
      <span>{props.label}</span>
      <button onClick={() => props.onChange(roundOffset(props.value - 0.1))}>
        <Minus size={16} />
      </button>
      <input
        value={draft}
        onChange={(event) => {
          setDraft(event.target.value);
          setInvalid(false);
        }}
        onBlur={() => commit(draft)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            commit(draft);
          }
          if (event.key === "Escape") {
            setDraft(props.value.toFixed(1));
            setInvalid(false);
          }
        }}
      />
      <em>s</em>
      <button onClick={() => props.onChange(roundOffset(props.value + 0.1))}>
        <Plus size={16} />
      </button>
      {invalid && <small>请输入有效秒数</small>}
    </div>
  );
}

function AnimeEditModal(props: {
  anime: LocalAnimeEntryUi;
  onCancel: () => void;
  onSave: (draft: AnimeEditDraft) => void;
}) {
  const initialDraft: AnimeEditDraft = {
    title: props.anime.title,
    aliasText: props.anime.alias?.join(" / ") ?? "",
    year: props.anime.year ? String(props.anime.year) : "",
    type: props.anime.type ?? "",
    tagsText: props.anime.tags?.join(" / ") ?? "",
    description: props.anime.description ?? "",
    coverTone: props.anime.coverTone,
    notes: props.anime.notes ?? "",
  };
  const [draft, setDraft] = useState(initialDraft);
  const dirty = JSON.stringify(draft) !== JSON.stringify(initialDraft);

  function cancel() {
    if (dirty && !window.confirm("信息尚未保存，确认关闭？")) {
      return;
    }
    props.onCancel();
  }

  return (
    <div className="modal-backdrop">
      <div className="anime-edit-modal">
        <div className="modal-header">
          <div>
            <h2>编辑信息</h2>
            <p>仅维护本地元数据，不会联网抓取。</p>
          </div>
          <button className="ghost" onClick={cancel}>
            关闭
          </button>
        </div>
        <div className="edit-form">
          <EditField label="标题" value={draft.title} onChange={(title) => setDraft((current) => ({ ...current, title }))} />
          <EditField
            label="别名"
            value={draft.aliasText}
            onChange={(aliasText) => setDraft((current) => ({ ...current, aliasText }))}
          />
          <EditField label="年份" value={draft.year} onChange={(year) => setDraft((current) => ({ ...current, year }))} />
          <EditField label="类型" value={draft.type} onChange={(type) => setDraft((current) => ({ ...current, type }))} />
          <EditField
            label="标签"
            value={draft.tagsText}
            onChange={(tagsText) => setDraft((current) => ({ ...current, tagsText }))}
          />
          <EditField
            label="封面色调"
            value={draft.coverTone}
            onChange={(coverTone) => setDraft((current) => ({ ...current, coverTone }))}
          />
          <label>
            <span>简介</span>
            <textarea
              value={draft.description}
              onChange={(event) => setDraft((current) => ({ ...current, description: event.target.value }))}
            />
          </label>
          <label>
            <span>备注</span>
            <textarea
              value={draft.notes}
              onChange={(event) => setDraft((current) => ({ ...current, notes: event.target.value }))}
            />
          </label>
        </div>
        <div className="modal-actions">
          <button className="secondary" onClick={cancel}>
            取消
          </button>
          <button className="primary" onClick={() => props.onSave(draft)}>
            保存
          </button>
        </div>
      </div>
    </div>
  );
}

function EditField(props: { label: string; value: string; onChange: (value: string) => void }) {
  return (
    <label>
      <span>{props.label}</span>
      <input value={props.value} onChange={(event) => props.onChange(event.target.value)} />
    </label>
  );
}

function CoverBlock({ entry, compact }: { entry: LocalAnimeEntryUi; compact?: boolean }) {
  return (
    <div className={`cover-block ${compact ? "compact" : ""} tone-${entry.coverTone}`}>
      <span>{entry.title.slice(0, 2).toUpperCase()}</span>
    </div>
  );
}

function InfoGrid({ label, value }: { label: string; value?: string }) {
  if (!value) {
    return null;
  }
  return (
    <div className="anime-info-row">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="info-row">
      <span>{label}</span>
      <strong title={value}>{value}</strong>
    </div>
  );
}

function WatchLegend({ status }: { status: WatchStatus }) {
  return (
    <span className={`watch-legend ${status}`}>
      <i />
      {watchStatusLabel[status]}
    </span>
  );
}

function WatchStatusBadge({ status }: { status: WatchStatus }) {
  return <span className={`watch-status ${status}`}>{watchStatusLabel[status]}</span>;
}

function EmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div className="empty-state">
      <strong>{title}</strong>
      <span>{body}</span>
    </div>
  );
}

function CandidateChips({ item }: { item: EpisodeMatch }) {
  const chips = unique(
    item.candidates.flatMap((candidate) => [
      languageLabels[candidate.language],
      candidate.extension.toUpperCase(),
    ]),
  ).slice(0, 6);

  if (chips.length === 0) {
    return <span className="muted">无候选</span>;
  }

  return (
    <div className="candidate-chip-row">
      {chips.map((chip) => (
        <span className={`mini-chip ${chipClass(chip)}`} key={chip}>
          {chip}
        </span>
      ))}
    </div>
  );
}

function StatusChip({ status }: { status: MatchStatus }) {
  return <span className={`chip ${status}`}>{statusLabel[status]}</span>;
}

function DetailDrawer(props: {
  allVideos: ScannedVideo[];
  item: EpisodeMatch | null;
  draft: DrawerDraft;
  isOpen: boolean;
  setDraft: React.Dispatch<React.SetStateAction<DrawerDraft>>;
  onApply: () => void;
  onClose: () => void;
}) {
  const item = props.item;
  const subtitleOptions = item?.candidates ?? [];

  return (
    <aside className={`detail-drawer ${props.isOpen ? "open" : ""}`} aria-hidden={!props.isOpen}>
      <div className="drawer-title">
        <button className="round-icon" onClick={props.onClose} aria-label="关闭详情">
          <X size={18} />
        </button>
        <h2>{item?.episodeKey ?? "--"} 详情</h2>
      </div>

      {!item ? (
        <div className="drawer-empty">请选择一行扫描结果。</div>
      ) : (
        <>
          <DrawerPanel title="视频信息" icon="header_video_info.svg">
            <InfoRow label="文件名" value={item.video?.fileName ?? "缺失视频"} />
            <InfoRow label="分辨率" value="待解析" />
            <InfoRow label="时长" value="待解析" />
            <InfoRow label="文件大小" value={formatBytes(item.video?.fileSizeBytes ?? 0)} />
            <InfoRow label="编码格式" value="待解析" />
          </DrawerPanel>

          <DrawerPanel title="字幕候选" icon="header_subtitle_candidate.svg">
            <div className="candidate-list">
              {subtitleOptions.length === 0 ? (
                <span className="muted">当前集没有字幕候选。</span>
              ) : (
                subtitleOptions.map((candidate, index) => (
                  <CandidateRow
                    candidate={candidate}
                    recommended={index === 0 || candidate.path === item.primarySubtitle?.path}
                    key={candidate.path}
                  />
                ))
              )}
            </div>
          </DrawerPanel>

          <DrawerPanel title="手动修正" icon="header_action_buttons.svg">
            <label className="fieldline">
              <span>视频集数</span>
              <input
                value={props.draft.episodeKey}
                onChange={(event) => props.setDraft((current) => ({ ...current, episodeKey: event.target.value }))}
              />
            </label>
            <label className="fieldline">
              <span>视频文件</span>
              <select
                value={props.draft.videoPath}
                onChange={(event) => props.setDraft((current) => ({ ...current, videoPath: event.target.value }))}
              >
                <option value="">缺失视频</option>
                {props.allVideos.map((video) => (
                  <option value={video.path} key={video.path}>
                    {video.fileName}
                  </option>
                ))}
              </select>
            </label>
            <label className="fieldline">
              <span>主字幕候选</span>
              <select
                value={props.draft.primarySubtitlePath}
                onChange={(event) =>
                  props.setDraft((current) => ({ ...current, primarySubtitlePath: event.target.value }))
                }
              >
                <option value="">不指定</option>
                {subtitleOptions.map((candidate) => (
                  <option value={candidate.path} key={candidate.path}>
                    {candidate.fileName}
                  </option>
                ))}
              </select>
            </label>
            <label className="fieldline">
              <span>副字幕候选</span>
              <select
                value={props.draft.secondarySubtitlePath}
                onChange={(event) =>
                  props.setDraft((current) => ({ ...current, secondarySubtitlePath: event.target.value }))
                }
              >
                <option value="">不指定</option>
                {subtitleOptions.map((candidate) => (
                  <option value={candidate.path} key={candidate.path}>
                    {candidate.fileName}
                  </option>
                ))}
              </select>
            </label>
            <button className="apply" onClick={props.onApply}>
              应用修正
            </button>
          </DrawerPanel>

          {item.status === "conflict" && (
            <DrawerPanel title="冲突说明" icon="status_warning_alert.svg" danger>
              <p className="conflict-text">
                检测到同一集存在多个可能的字幕版本，或同语言字幕候选过多。请确认最终保留的字幕版本。
              </p>
              {item.notes.map((note) => (
                <p className="conflict-note" key={note}>
                  {note}
                </p>
              ))}
            </DrawerPanel>
          )}

          <DrawerPanel title="备注" icon="header_action_buttons.svg">
            <textarea
              className="note-box"
              value={props.draft.note}
              onChange={(event) => props.setDraft((current) => ({ ...current, note: event.target.value }))}
              placeholder="在此输入备注信息..."
            />
          </DrawerPanel>
        </>
      )}
    </aside>
  );
}

function DrawerPanel(props: {
  title: string;
  icon: string;
  danger?: boolean;
  children: React.ReactNode;
}) {
  return (
    <section className={`drawer-panel ${props.danger ? "danger" : ""}`}>
      <h3>
        <img src={asset(`icons/${props.icon}`)} alt="" />
        {props.title}
      </h3>
      {props.children}
    </section>
  );
}

function CandidateRow({ candidate, recommended }: { candidate: SubtitleCandidate; recommended: boolean }) {
  return (
    <div className="candidate-row">
      <div className="candidate-main">
        <span className="mini-chip">{languageLabels[candidate.language]}</span>
        <span className="mini-chip">{candidate.extension.toUpperCase()}</span>
        {recommended && <span className="recommend">推荐</span>}
      </div>
      <span title={candidate.path}>{candidate.path}</span>
    </div>
  );
}

function PlanModal(props: {
  plan: OrganizePlan;
  setPlan: (plan: OrganizePlan | null) => void;
  updateCollision: (index: number, action: CollisionAction) => void;
  executePlan: () => void;
}) {
  return (
    <div className="modal-backdrop">
      <div className="plan-modal">
        <div className="modal-header">
          <div>
            <h2>{props.plan.mode === "copy" ? "复制整理确认" : "移动整理确认"}</h2>
            <p>
              视频 {props.plan.summary.videos} 个，字幕 {props.plan.summary.subtitles} 个，冲突{" "}
              {props.plan.summary.conflicts} 个
            </p>
          </div>
          <button className="ghost" onClick={() => props.setPlan(null)}>
            关闭
          </button>
        </div>
        <div className="plan-list">
          {props.plan.items.map((item, index) => (
            <div className="plan-item" key={`${item.source}-${index}`}>
              <div>
                <strong>{item.episodeKey}</strong>
                <span>{item.source}</span>
                <small>→ {item.destination}</small>
              </div>
              {item.collision ? (
                <select
                  value={item.collisionAction}
                  onChange={(event) => props.updateCollision(index, event.target.value as CollisionAction)}
                >
                  <option value="skip">跳过</option>
                  <option value="replace">替换</option>
                  <option value="rename">重命名</option>
                </select>
              ) : (
                <span className="ready">可执行</span>
              )}
            </div>
          ))}
        </div>
        <div className="modal-actions">
          <button className="secondary" onClick={() => props.setPlan(null)}>
            取消
          </button>
          <button className="primary" onClick={props.executePlan}>
            确认执行
          </button>
        </div>
      </div>
    </div>
  );
}

function getEffectiveStatus(item: EpisodeMatch, expectedSubtitleCount: number): MatchStatus {
  if (item.status === "conflict") {
    return "conflict";
  }
  if (!item.video) {
    return "missingVideo";
  }
  if (item.candidates.length < expectedSubtitleCount) {
    return "missingSub";
  }
  if (item.status === "unprocessed" || item.status === "pendingFix") {
    return item.status;
  }
  return "matched";
}

function parseEpisodeKey(value: string): EpisodeKey | null {
  const match = value.trim().match(/^S(\d{1,2})E(\d{1,3})$/i);
  if (!match) {
    return null;
  }
  return {
    season: Number(match[1]),
    episode: Number(match[2]),
  };
}

function formatEpisodeKey(key: EpisodeKey) {
  return `S${String(key.season).padStart(2, "0")}E${String(key.episode).padStart(2, "0")}`;
}

function outputPreview(projectName: string, season: string) {
  return `${projectName} ${season}/
├─ videos/
│  └─ ${projectName} S01E01.mkv
├─ subs/
│  ├─ zh-Hans/
│  │  └─ ${projectName} S01E01.zh-Hans.ass
│  └─ ja/
│     └─ ${projectName} S01E01.ja.srt
└─ anime-sub-map.json（自动生成）`;
}

function formatBytes(bytes: number) {
  if (bytes <= 0) {
    return "未知";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value.toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}

function formatDuration(seconds?: number) {
  if (!seconds || seconds <= 0) {
    return "未知";
  }
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
  }
  return `${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
}

function formatPlaybackProgress(episode: LocalEpisode | null) {
  if (!episode?.lastPositionSec) {
    return "未开始";
  }
  return `${formatDuration(episode.lastPositionSec)} (${episode.progressPercent ?? 0}%)`;
}

function fileNameFromPath(path?: string) {
  if (!path) {
    return "";
  }
  return path.split(/[\\/]/).pop() ?? path;
}

function animeProgress(entry: LocalAnimeEntryUi) {
  if (entry.episodes.length === 0) {
    return 0;
  }
  const lastIndex = entry.episodes.findIndex((episode) => episode.id === entry.lastWatchedEpisodeId);
  if (lastIndex >= 0) {
    return Math.min(100, Math.floor(((lastIndex + 1) / entry.episodes.length) * 100));
  }
  const watchedScore = entry.episodes.reduce((sum, episode) => sum + (episode.progressPercent ?? 0), 0);
  return Math.floor(watchedScore / entry.episodes.length);
}

function normalizeDirectoryKey(path: string) {
  return path.trim().replace(/[\\/]+$/, "").toLowerCase();
}

function roundOffset(value: number) {
  return Math.round(value * 10) / 10;
}

function splitTextList(value: string) {
  return value
    .split(/[、,，/]+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function chipClass(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-");
}

function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

function unique<T>(values: T[]) {
  return [...new Set(values)];
}

function selectDefaultAnimeId(entries: LocalAnimeEntryUi[]) {
  return entries.find((entry) => entry.lastWatchedEpisodeId)?.id ?? entries[0]?.id;
}

function makeDemoAnimeLibrary(): LocalAnimeEntryUi[] {
  return [
    makeAnime("jjk-s1", "Jujutsu Kaisen S01", "咒术回战 第一季", "2020", 24, 3, "violet", [
      "热血",
      "校园",
      "战斗",
      "奇幻",
    ]),
    makeAnime("jjk-s2", "Jujutsu Kaisen S02", "咒术回战 第二季", "2023", 23, 5, "blue", ["热血", "战斗"]),
    makeAnime("demon-slayer-s3", "Demon Slayer S03", "鬼灭之刃 锻刀村篇", "2023", 11, 2, "red", ["战斗", "奇幻"]),
    makeAnime("aot-s4p2", "Attack on Titan S04 Part2", "进击的巨人 最终季", "2022", 12, 2, "amber", [
      "剧情",
      "动作",
    ]),
    makeAnime("steins-gate", "Steins;Gate", "命运石之门", "2011", 24, 1, "green", ["科幻", "悬疑"]),
    makeAnime("spirited-away", "Spirited Away", "千与千寻", "2001", 1, 1, "pink", ["电影", "奇幻"]),
    makeAnime("your-name", "Your Name.", "你的名字。", "2016", 1, 1, "sky", ["电影", "青春"]),
    makeAnime("frieren", "Frieren: Beyond Journey's End", "葬送的芙莉莲", "2023", 28, 8, "sky", [
      "奇幻",
      "冒险",
    ]),
    makeAnime("spy-family-s1", "Spy x Family S01", "间谍过家家 第一季", "2022", 25, 16, "green", [
      "喜剧",
      "日常",
    ]),
    makeAnime("vinland-saga-s2", "Vinland Saga S02", "冰海战记 第二季", "2023", 24, 7, "amber", [
      "剧情",
      "历史",
    ]),
    makeAnime("chainsaw-man", "Chainsaw Man", "链锯人", "2022", 12, 5, "red", ["战斗", "奇幻"]),
    makeAnime("suzume", "Suzume", "铃芽之旅", "2022", 1, 1, "violet", ["电影", "奇幻"]),
  ];
}

function makeAnime(
  id: string,
  title: string,
  alias: string,
  year: string,
  episodeCount: number,
  lastWatchedEpisode: number,
  coverTone: string,
  tags: string[],
): LocalAnimeEntryUi {
  const episodes = Array.from({ length: episodeCount }, (_, index) =>
    makeEpisode(id, index + 1, index + 1 < lastWatchedEpisode ? "watched" : index + 1 === lastWatchedEpisode ? "partial" : "unwatched"),
  );
  return {
    id,
    title,
    alias: [alias],
    year: Number(year),
    type: episodeCount === 1 ? "Movie" : "TV 动画",
    tags,
    description:
      id === "jjk-s1"
        ? "少年虎杖悠仁吞下特级咒物「两面宿傩」的手指，因而获得强大力量并被卷入咒术师的世界。为了拯救同伴并对抗诅咒，他选择踏上成为咒术师的道路。"
        : "暂无简介，可点击编辑信息补充。",
    coverTone,
    rootDir: `D:\\Anime Library\\${title}`,
    videoDir: `D:\\Anime Library\\${title}\\videos`,
    subtitleDirs: [`D:\\Anime Library\\${title}\\subs\\zh-Hans`, `D:\\Anime Library\\${title}\\subs\\en`],
    subtitleLanguages: ["zh-Hans", "en"],
    episodes,
    lastWatchedEpisodeId: episodes[Math.max(0, lastWatchedEpisode - 1)]?.id,
    createdAt: "2026-05-01T08:00:00.000Z",
    updatedAt: "2026-05-22T09:30:00.000Z",
  };
}

function makeEpisode(animeId: string, episode: number, watchStatus: WatchStatus): LocalEpisode {
  const episodeKey = `S01E${String(episode).padStart(2, "0")}`;
  return {
    id: `${animeId}-${episodeKey}`,
    episodeKey,
    title: episodeTitle(episode),
    videoPath: `D:\\Anime Library\\${animeId}\\videos\\Jujutsu Kaisen ${episodeKey}.mkv`,
    durationSec: 1435,
    resolution: "1920 × 1080 (16:9)",
    codec: "HEVC / AAC",
    fileSizeBytes: 324_600_000,
    subtitles: [
      {
        id: `${animeId}-${episodeKey}-zh`,
        path: `D:\\Anime Library\\${animeId}\\subs\\zh-Hans\\${episodeKey}.zh-Hans.ass`,
        language: "zh-Hans",
        format: "ass",
        role: "primary",
      },
      {
        id: `${animeId}-${episodeKey}-en`,
        path: `D:\\Anime Library\\${animeId}\\subs\\en\\${episodeKey}.en.srt`,
        language: "en",
        format: "srt",
        role: "secondary",
      },
    ],
    watchStatus,
    lastPositionSec: watchStatus === "partial" ? 178 : watchStatus === "watched" ? 1435 : undefined,
    progressPercent: watchStatus === "watched" ? 100 : watchStatus === "partial" ? 12 : 0,
  };
}

function episodeTitle(episode: number) {
  const titles = ["两面宿傩", "自称是咒术师的人", "铁骨娘娘", "咒胎戴天 - 壹 -", "咒胎戴天 - 贰 -", "雨后", "急袭", "退屈"];
  return titles[episode - 1] ?? `第 ${episode} 集`;
}

function makeDemoScanResult(): ScanAndMatchResult {
  const videos = Array.from({ length: 6 }, (_, index) => makeDemoVideo(index + 1));
  const subtitles: ScannedSubtitle[] = videos.flatMap((video, index) => {
    const episode = index + 1;
    if (episode === 6) {
      return [makeDemoSubtitle(episode, "zh-Hans", "ass"), makeDemoSubtitle(episode, "ja", "srt")];
    }
    if (episode === 4) {
      return [
        makeDemoSubtitle(episode, "ja", "srt"),
        makeDemoSubtitle(episode, "en", "srt"),
        makeDemoSubtitle(episode, "zh-Hans", "ass"),
        makeDemoSubtitle(episode, "zh-Hans", "srt"),
      ];
    }
    return [
      makeDemoSubtitle(episode, "zh-Hans", "ass"),
      makeDemoSubtitle(episode, "ja", "srt"),
      makeDemoSubtitle(episode, "en", "srt"),
    ];
  });

  const matches: EpisodeMatch[] = videos.map((video, index) => {
    const episode = index + 1;
    const episodeSubtitles = subtitles.filter((subtitle) => subtitle.episodeKey === video.episodeKey);
    const candidates = episodeSubtitles.map((subtitle) => ({
      path: subtitle.path,
      fileName: subtitle.fileName,
      extension: subtitle.extension,
      language: subtitle.language,
      confidence: subtitle.confidence,
      role: "candidate" as const,
    }));
    return {
      episode: video.episode ?? { season: 1, episode },
      episodeKey: video.episodeKey ?? `S01E${String(episode).padStart(2, "0")}`,
      video,
      primarySubtitle: candidates.find((candidate) => candidate.language === "zh-Hans") ?? null,
      secondarySubtitle: candidates.find((candidate) => candidate.language === "ja") ?? null,
      candidates,
      status: episode === 4 ? "conflict" : episode === 6 ? "missingSub" : "matched",
      notes: episode === 4 ? ["zh-Hans.ass 与 zh-Hans.srt 同时匹配，需要手动确认。"] : [],
    };
  });

  return {
    scan: { videos, subtitles },
    matches,
    unprocessedVideos: [],
    unprocessedSubtitles: [],
  };
}

function makeDemoVideo(episode: number): ScannedVideo {
  const key = `S01E${String(episode).padStart(2, "0")}`;
  return {
    path: `D:\\Anime\\Jujutsu Kaisen\\videos\\Jujutsu Kaisen ${key}.mkv`,
    fileName: `Jujutsu Kaisen ${key}.mkv`,
    extension: "mkv",
    fileSizeBytes: 1_240_000_000,
    episode: { season: 1, episode },
    episodeKey: key,
    confidence: 100,
  };
}

function makeDemoSubtitle(episode: number, language: LanguageCode, extension: string): ScannedSubtitle {
  const key = `S01E${String(episode).padStart(2, "0")}`;
  return {
    path: `D:\\subs_${language}\\${key}.${language}.${extension}`,
    fileName: `${key}.${language}.${extension}`,
    extension,
    fileSizeBytes: 120_000,
    episode: { season: 1, episode },
    episodeKey: key,
    confidence: 98,
    language,
  };
}

function makeDemoPlan(
  projectName: string,
  season: string,
  outputDir: string,
  mode: OrganizeMode,
  matches: EpisodeMatch[],
): OrganizePlan {
  const items = matches.flatMap((item) => {
    const videoItem = item.video
      ? [
          {
            source: item.video.path,
            destination: `${outputDir}\\videos\\${projectName} ${item.episodeKey}.${item.video.extension}`,
            kind: "video" as const,
            episodeKey: item.episodeKey,
            language: null,
            role: null,
            collision: false,
            collisionAction: "skip" as const,
            status: "planned" as const,
            message: null,
          },
        ]
      : [];
    const subItems = item.candidates.slice(0, 2).map((candidate) => ({
      source: candidate.path,
      destination: `${outputDir}\\subs\\${candidate.language}\\${projectName} ${item.episodeKey}.${candidate.language}.${candidate.extension}`,
      kind: "subtitle" as const,
      episodeKey: item.episodeKey,
      language: candidate.language,
      role: candidate.role,
      collision: false,
      collisionAction: "skip" as const,
      status: "planned" as const,
      message: null,
    }));
    return [...videoItem, ...subItems];
  });

  return {
    projectName,
    season,
    outputDir,
    mode,
    items,
    hasConflicts: false,
    mapFilePath: `${outputDir}\\anime-sub-map.json`,
    mapFileExists: false,
    summary: {
      videos: matches.filter((item) => item.video).length,
      subtitles: items.filter((item) => item.kind === "subtitle").length,
      conflicts: 0,
    },
    projectMap: {},
  };
}

export default App;
