import { listen } from "@tauri-apps/api/event";
import { Check, ChevronLeft, ChevronRight, Copy, Edit3, FolderOpen, Info, Loader2, Minimize2, RefreshCcw, Rocket, Save } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  buildOrganizePlan,
  executeOrganizePlan,
  loadAppSettings,
  saveLocalLibraryEntry,
  saveParseTrainingSample,
  scanAndMatch as scanAndMatchCommand,
  selectDirectories,
  selectDirectory,
} from "../../services/tauriCommands";
import {
  asset,
  browserPreviewMessage,
  chipClass,
  formatBytes,
  isTauriRuntime,
  languageLabels,
  unique,
} from "../../shared/utils";
import type {
  AppSettings,
  CollisionAction,
  EpisodeKey,
  EpisodeMatch,
  LanguageCode,
  LocalAnimeLibraryEntry,
  MatchStatus,
  OrganizeExecutionResult,
  OrganizeMode,
  OrganizePlan,
  OrganizeProgressEvent,
  SaveLocalLibraryRequest,
  ScanAndMatchResult,
  ScannedSubtitle,
  ScannedVideo,
  SubtitleCandidate,
} from "../../types";

type ScanState = "idle" | "scanning" | "ready";
type DrawerMode = "closed" | "episodeDetail";
type DirectoryAction = "video" | "subtitles" | "output";

interface DrawerDraft {
  episodeKey: string;
  videoPath: string;
  primarySubtitlePath: string;
  secondarySubtitlePath: string;
  note: string;
}

interface DrawerState {
  mode: DrawerMode;
  episodeKey: string | null;
}

interface ProjectHomeWorkflowState {
  projectName: string;
  season: string;
  videoDir: string | null;
  subtitleDirs: string[];
  outputDir: string | null;
  scanState: ScanState;
  drawer: DrawerState;
  organizeMode: OrganizeMode;
}

const statusLabel: Record<MatchStatus, string> = {
  matched: "已匹配",
  pendingFix: "待修正",
  conflict: "冲突",
  unprocessed: "未处理",
  missingVideo: "缺失视频",
  missingSub: "未完整",
};

const parseStatusLabel = {
  accepted: "已确认",
  lowConfidence: "低置信",
  ambiguous: "需确认",
  rejected: "未识别",
} as const;

const fallbackAppSettings: AppSettings = {
  schemaVersion: 1,
  mpvExecutablePath: "mpv",
  defaultOutputDir: "D:\\整理输出",
  animeLibraryRootDir: "D:\\AnimeLibrary",
  tempDir: "C:\\Users\\User\\AppData\\Local\\mpv_tidy\\temp",
  defaultPrimarySubtitleLanguage: "zh-Hans",
  defaultSecondarySubtitleLanguage: "ja",
  rememberPlaybackProgress: true,
  autoScanAnimeLibraryOnStartup: true,
  autoSaveWatchProgress: true,
  defaultCoverStrategy: "local-first-then-screenshot",
  updatedAtUnix: 0,
};

export function useProjectHomeWorkflow() {
  const [drawer, setDrawer] = useState<DrawerState>({ mode: "closed", episodeKey: null });

  function openDetailDrawer(episodeKey: string | null) {
    setDrawer({ mode: episodeKey ? "episodeDetail" : "closed", episodeKey });
  }

  function closeDetailDrawer(episodeKey: string | null) {
    setDrawer({ mode: "closed", episodeKey });
  }

  return { drawer, openDetailDrawer, closeDetailDrawer };
}

export interface OrganizeTaskUi {
  id: string;
  title: string;
  mode: OrganizeMode;
  status: "running" | "completed" | "failed";
  total: number;
  processed: number;
  message: string;
  currentDestination: string | null;
  startedAt: number;
  completedAt: number | null;
}

export function ProjectHomePage({
  showToast,
  onLibraryEntrySaved,
  organizeTasks,
  setOrganizeTasks,
}: {
  showToast: (message: string) => void;
  onLibraryEntrySaved: (entry: LocalAnimeLibraryEntry) => void;
  organizeTasks: OrganizeTaskUi[];
  setOrganizeTasks: React.Dispatch<React.SetStateAction<OrganizeTaskUi[]>>;
}) {
  const [projectName, setProjectName] = useState("Jujutsu Kaisen");
  const [projectNameEdited, setProjectNameEdited] = useState(false);
  const [season, setSeason] = useState("S01");
  const [videoDir, setVideoDir] = useState<string | null>(null);
  const [subtitleDirs, setSubtitleDirs] = useState<string[]>([]);
  const [outputDir, setOutputDir] = useState<string | null>(null);
  const [outputHistory, setOutputHistory] = useState<string[]>([]);
  const [scanState, setScanState] = useState<ScanState>("idle");
  const [scanResult, setScanResult] = useState<ScanAndMatchResult | null>(null);
  const [appSettings, setAppSettings] = useState<AppSettings>(fallbackAppSettings);
  const [selectedEpisodeKey, setSelectedEpisodeKey] = useState<string | null>(null);
  const { drawer, openDetailDrawer, closeDetailDrawer } = useProjectHomeWorkflow();
  const [organizeMode, setOrganizeMode] = useState<OrganizeMode>("copy");
  const [plan, setPlan] = useState<OrganizePlan | null>(null);
  const [completedPlan, setCompletedPlan] = useState<OrganizePlan | null>(null);
  const [organizedResult, setOrganizedResult] = useState<OrganizeExecutionResult | null>(null);
  const [isBuildingPlan, setIsBuildingPlan] = useState(false);
  const [isExecutingPlan, setIsExecutingPlan] = useState(false);
  const [organizeProgress, setOrganizeProgress] = useState<OrganizeProgressEvent | null>(null);
  const [librarySaved, setLibrarySaved] = useState(false);
  const [message, setMessage] = useState("请选择视频目录、字幕目录和输出目录，然后开始扫描。");
  const [pendingDirectoryAction, setPendingDirectoryAction] = useState<DirectoryAction | null>(null);
  const activeOrganizeTaskIdRef = useRef<string | null>(null);
  const [drawerDraft, setDrawerDraft] = useState<DrawerDraft>({
    episodeKey: "",
    videoPath: "",
    primarySubtitlePath: "",
    secondarySubtitlePath: "",
    note: "",
  });

  const matches = scanResult?.matches ?? [];
  const hasRunningOrganizeTask = organizeTasks.some((task) => task.status === "running");
  const expectedSubtitleCount = Math.max(1, subtitleDirs.length);
  const workflowState: ProjectHomeWorkflowState = {
    projectName,
    season,
    videoDir,
    subtitleDirs,
    outputDir,
    scanState,
    drawer,
    organizeMode,
  };
  const drawerOpen = workflowState.drawer.mode === "episodeDetail";

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
    if (!isTauriRuntime()) {
      setOutputDir((current) => current ?? fallbackAppSettings.defaultOutputDir);
      return;
    }
    loadAppSettings()
      .then((loaded) => {
        setAppSettings(loaded);
        setOutputDir((current) => current ?? loaded.defaultOutputDir);
      })
      .catch((error) => {
        setMessage(`设置读取失败：${String(error)}`);
      });
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let disposed = false;
    let unlisten: (() => void) | null = null;
    listen<OrganizeProgressEvent>("organize-progress", (event) => {
      if (!disposed) {
        setOrganizeProgress(event.payload);
        const taskId = activeOrganizeTaskIdRef.current;
        if (taskId) {
          setOrganizeTasks((current) =>
            current.map((task) =>
              task.id === taskId
                ? {
                    ...task,
                    total: event.payload.total,
                    processed: event.payload.processed,
                    message: event.payload.message,
                    currentDestination: event.payload.currentDestination,
                  }
                : task,
            ),
          );
        }
      }
    })
      .then((cleanup) => {
        if (disposed) {
          cleanup();
          return;
        }
        unlisten = cleanup;
      })
      .catch((error) => {
        setMessage(`整理进度监听启动失败：${String(error)}`);
      });

    return () => {
      disposed = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

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
    setPendingDirectoryAction("video");
    setMessage("正在打开视频目录选择器...");
    try {
      const selected = await selectDirectory();
      if (selected) {
        setVideoDir(selected);
        if (!projectNameEdited) {
          setProjectName(inferProjectNameFromVideoDir(selected));
        }
        setScanResult(null);
        setScanState("idle");
        setSelectedEpisodeKey(null);
        closeDetailDrawer(null);
        setMessage("已更新视频目录。");
      }
    } catch (error) {
      setMessage(`视频目录选择失败：${String(error)}`);
    } finally {
      setPendingDirectoryAction(null);
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
    setPendingDirectoryAction("subtitles");
    setMessage("正在打开字幕目录选择器...");
    try {
      const paths = await selectDirectories();
      if (paths.length === 0) {
        return;
      }
      setSubtitleDirs((current) => unique([...current, ...paths]));
      setScanResult(null);
      setScanState("idle");
      closeDetailDrawer(null);
      setMessage(`已添加 ${paths.length} 个字幕目录。`);
    } catch (error) {
      setMessage(`字幕目录选择失败：${String(error)}`);
    } finally {
      setPendingDirectoryAction(null);
    }
  }

  function clearSubtitleDirs() {
    if (subtitleDirs.length === 0) {
      return;
    }
    setSubtitleDirs([]);
    setScanResult(null);
    setScanState("idle");
    setSelectedEpisodeKey(null);
    closeDetailDrawer(null);
    setMessage("已清空字幕目录。");
  }

  async function chooseOutputDir() {
    if (!isTauriRuntime()) {
      const sample = "D:\\整理输出\\Jujutsu Kaisen S01";
      setOutputDir(sample);
      setOutputHistory((current) => unique([sample, ...current]).slice(0, 3));
      setMessage(browserPreviewMessage);
      return;
    }
    setPendingDirectoryAction("output");
    setMessage("正在打开输出目录选择器...");
    try {
      const selected = await selectDirectory();
      if (selected) {
        setOutputDir(selected);
        setOutputHistory((current) => unique([selected, ...current]).slice(0, 3));
        setMessage("已更新输出目录。");
      }
    } catch (error) {
      setMessage(`输出目录选择失败：${String(error)}`);
    } finally {
      setPendingDirectoryAction(null);
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
        const nextEpisodeKey = demo.matches[2]?.episodeKey ?? demo.matches[0]?.episodeKey ?? null;
        setSelectedEpisodeKey(nextEpisodeKey);
        openDetailDrawer(nextEpisodeKey);
        setScanState("ready");
        setMessage("浏览器预览已加载示例扫描结果。");
      }, 180);
      return;
    }

    try {
      const result = await scanAndMatchCommand({ videoDirs: [videoDir], subtitleDirs });
      const nextEpisodeKey = result.matches[0]?.episodeKey ?? null;
      if (!projectNameEdited) {
        setProjectName(inferProjectNameFromScanResult(result) ?? inferProjectNameFromVideoDir(videoDir));
      }
      setScanResult(result);
      setSelectedEpisodeKey(nextEpisodeKey);
      openDetailDrawer(nextEpisodeKey);
      setScanState("ready");
      setMessage(`扫描完成：${result.scan.videos.length} 个视频，${result.scan.subtitles.length} 个字幕。`);
    } catch (error) {
      setScanState(scanResult ? "ready" : "idle");
      setMessage(String(error));
    }
  }

  async function startOrganize() {
    if (isBuildingPlan || isExecutingPlan) {
      return;
    }
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

    setIsBuildingPlan(true);
    try {
      const result = await buildOrganizePlan({
        projectName,
        season,
        outputDir,
        matches,
        mode: organizeMode,
        primaryLanguage: appSettings.defaultPrimarySubtitleLanguage,
        secondaryLanguage: appSettings.defaultSecondarySubtitleLanguage,
      });
      setPlan(result);
      setOrganizeProgress(null);
      setMessage(result.hasConflicts ? "整理计划已生成，存在冲突项，请先确认处理方式。" : "整理计划已生成，请确认执行。");
    } catch (error) {
      setMessage(String(error));
    } finally {
      setIsBuildingPlan(false);
    }
  }

  async function executePlan() {
    if (!plan || isExecutingPlan) {
      return;
    }
    const taskId = `organize-${Date.now()}`;
    activeOrganizeTaskIdRef.current = taskId;
    setOrganizeTasks((current) => [
      {
        id: taskId,
        title: `${plan.projectName} ${plan.season}`,
        mode: plan.mode,
        status: "running",
        total: plan.items.length,
        processed: 0,
        message: "整理任务已创建。",
        currentDestination: null,
        startedAt: Date.now(),
        completedAt: null,
      },
      ...current,
    ]);

    if (!isTauriRuntime()) {
      setIsExecutingPlan(true);
      setOrganizeProgress({
        total: plan.items.length,
        processed: plan.items.length,
        currentEpisodeKey: null,
        currentDestination: plan.mapFilePath,
        status: "copied",
        message: "浏览器预览已模拟整理完成。",
      });
      setOrganizedResult({
        items: plan.items,
        mapWritten: true,
        message: plan.mode === "copy" ? "示例复制整理完成。" : "示例移动整理完成。",
      });
      setCompletedPlan(plan);
      setPlan(null);
      setLibrarySaved(false);
      setIsExecutingPlan(false);
      setOrganizeTasks((current) =>
        current.map((task) =>
          task.id === taskId
            ? {
                ...task,
                status: "completed",
                processed: task.total,
                message: "浏览器预览已模拟整理完成。",
                currentDestination: plan.mapFilePath,
                completedAt: Date.now(),
              }
            : task,
        ),
      );
      activeOrganizeTaskIdRef.current = null;
      setMessage("浏览器预览已模拟整理完成。");
      return;
    }

    setIsExecutingPlan(true);
    setOrganizeProgress({
      total: plan.items.length,
      processed: 0,
      currentEpisodeKey: null,
      currentDestination: null,
      status: "planned",
      message: "整理任务已提交，正在准备文件操作。",
    });
    try {
      const result = await executeOrganizePlan(plan);
      setOrganizedResult(result);
      setCompletedPlan(plan);
      setPlan(null);
      setLibrarySaved(false);
      setOutputHistory((current) => unique([plan.outputDir, ...current]).slice(0, 3));
      setOrganizeTasks((current) =>
        current.map((task) =>
          task.id === taskId
            ? {
                ...task,
                status: "completed",
                processed: task.total,
                message: result.message,
                currentDestination: plan.mapFilePath,
                completedAt: Date.now(),
              }
            : task,
        ),
      );
      setMessage(result.message);
    } catch (error) {
      setOrganizeTasks((current) =>
        current.map((task) =>
          task.id === taskId
            ? {
                ...task,
                status: "failed",
                message: String(error),
                completedAt: Date.now(),
              }
            : task,
        ),
      );
      setMessage(String(error));
    } finally {
      setIsExecutingPlan(false);
      activeOrganizeTaskIdRef.current = null;
    }
  }

  function minimizeOrganizeTask() {
    setPlan(null);
    setMessage("整理任务已缩小到右下角任务列表，可继续浏览其他页面。");
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
      subtitlePreferenceSnapshot: {
        primaryLanguage: appSettings.defaultPrimarySubtitleLanguage,
        secondaryLanguage: appSettings.defaultSecondarySubtitleLanguage,
      },
      coverStrategySnapshot: appSettings.defaultCoverStrategy,
      episodes: matches.map((item) => ({
        episodeKey: item.episodeKey,
        videoPath: organizedPathFor(organizedResult.items, item.episodeKey, "video", null) ?? item.video?.path ?? null,
        primarySubtitlePath:
          organizedPathFor(organizedResult.items, item.episodeKey, "subtitle", "primary") ?? item.primarySubtitle?.path ?? null,
        secondarySubtitlePath:
          organizedPathFor(organizedResult.items, item.episodeKey, "subtitle", "secondary") ?? item.secondarySubtitle?.path ?? null,
        subtitleCount: item.candidates.length,
        status: getEffectiveStatus(item, expectedSubtitleCount),
        watchStatus: "unwatched",
        lastPositionSec: null,
        progressPercent: null,
        updatedAtUnix: 0,
      })),
    };

    if (!isTauriRuntime()) {
      const saved = makePreviewLibraryEntry(request);
      setLibrarySaved(true);
      setMessage("浏览器预览已模拟保存到本地动漫。");
      onLibraryEntrySaved(saved);
      showToast("已保存到本地动漫");
      return;
    }

    try {
      const saved = await saveLocalLibraryEntry(request);
      setLibrarySaved(true);
      setMessage(`已保存到本地动漫：${saved.projectName} ${saved.season}`);
      onLibraryEntrySaved(saved);
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
    openDetailDrawer(item.episodeKey);
  }

  async function applyManualCorrection() {
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
    if (isTauriRuntime() && drawerDraft.videoPath) {
      try {
        await saveParseTrainingSample({
          path: drawerDraft.videoPath,
          confirmedEpisode: parsedEpisode,
          note: drawerDraft.note || "用户在项目首页手动修正 episode。",
        });
      } catch (error) {
        setMessage(`已应用修正，但训练样本保存失败：${String(error)}`);
        return;
      }
    }
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

  function updateAllCollisions(action: CollisionAction) {
    setPlan((current) => {
      if (!current) {
        return current;
      }
      return {
        ...current,
        items: current.items.map((item) => (item.collision ? { ...item, collisionAction: action } : item)),
      };
    });
  }

  return (
    <main className="workspace">
      <header className="topbar">
        <div className="project-title">
          <input
            aria-label="项目名称"
            value={projectName}
            onChange={(event) => {
              setProjectNameEdited(true);
              setProjectName(event.target.value);
            }}
          />
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
              busy={pendingDirectoryAction === "video"}
              onClick={chooseVideoDir}
            />
            <SubtitleDirectoryCard
              dirs={subtitleDirs}
              busy={pendingDirectoryAction === "subtitles"}
              onAdd={addSubtitleDirs}
              onClear={clearSubtitleDirs}
            />
            <DirectoryCard
              icon="status_folder.svg"
              title="输出目录"
              primaryText={outputDir ?? "尚未选择输出目录"}
              metaText={outputHistory[0] ? `最近：${outputHistory[0]}` : "最近整理：暂无记录"}
              buttonText="更换目录"
              busy={pendingDirectoryAction === "output"}
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
            <OrganizeModePicker mode={organizeMode} onSelect={selectMode} disabled={isBuildingPlan || isExecutingPlan} />
            <button className="op-button organize" disabled={isBuildingPlan || isExecutingPlan || hasRunningOrganizeTask} onClick={startOrganize}>
              {isBuildingPlan ? <Loader2 className="spin" size={20} /> : <Rocket size={20} />}
              {isBuildingPlan ? "生成计划中..." : hasRunningOrganizeTask ? "整理任务进行中" : "开始整理"}
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

      </section>

      <DetailDrawer
        allVideos={scanResult?.scan.videos ?? []}
        item={selectedMatch}
        draft={drawerDraft}
        isOpen={drawerOpen}
        setDraft={setDrawerDraft}
        onApply={applyManualCorrection}
        onClose={() => closeDetailDrawer(selectedEpisodeKey)}
      />
      {!drawerOpen && selectedEpisodeKey && (
        <button className="drawer-expand-tab" aria-label="展开详情" onClick={() => openDetailDrawer(selectedEpisodeKey)}>
          <ChevronLeft size={18} />
        </button>
      )}

      <div className="statusbar">{message}</div>

      {plan && (
        <PlanModal
          plan={plan}
          setPlan={setPlan}
          updateCollision={updateCollision}
          updateAllCollisions={updateAllCollisions}
          executePlan={executePlan}
          isExecuting={isExecutingPlan}
          progress={organizeProgress}
          onMinimize={minimizeOrganizeTask}
        />
      )}
    </main>
  );
}

function OrganizeModePicker(props: {
  mode: OrganizeMode;
  disabled: boolean;
  onSelect: (mode: OrganizeMode) => void;
}) {
  const options = [
    {
      mode: "copy" as const,
      icon: <Copy size={17} />,
      title: "复制",
      description: "保留原文件",
    },
    {
      mode: "move" as const,
      icon: <FolderOpen size={17} />,
      title: "移动",
      description: "整理后移走源文件",
    },
  ];

  return (
    <div className="organize-mode-picker" role="radiogroup" aria-label="整理方式">
      {options.map((option) => (
        <label className={`organize-mode-option ${props.mode === option.mode ? "active" : ""}`} key={option.mode}>
          <input
            type="radio"
            name="organize-mode"
            value={option.mode}
            checked={props.mode === option.mode}
            disabled={props.disabled}
            onChange={() => props.onSelect(option.mode)}
          />
          <span className="mode-check" aria-hidden="true" />
          <span className="mode-icon" aria-hidden="true">
            {option.icon}
          </span>
          <span className="mode-copy">
            <strong>{option.title}</strong>
            <small>{option.description}</small>
          </span>
        </label>
      ))}
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
  busy?: boolean;
  onClick: () => void;
}) {
  return (
    <div className="resource-card">
      <img src={asset(`icons/${props.icon}`)} alt="" />
      <div>
        <h3>{props.title}</h3>
        <p title={props.primaryText}>{props.primaryText}</p>
        <small title={props.metaText}>{props.metaText}</small>
        <button className={props.accent ? "pink-outline" : "violet-outline"} disabled={props.busy} onClick={props.onClick}>
          {props.busy && <Loader2 className="spin" size={15} />}
          {props.busy ? "正在打开..." : props.buttonText}
        </button>
      </div>
    </div>
  );
}

function SubtitleDirectoryCard({
  busy = false,
  dirs,
  onAdd,
  onClear,
}: {
  busy?: boolean;
  dirs: string[];
  onAdd: () => void;
  onClear: () => void;
}) {
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
        <div className="directory-actions">
          <button className="violet-outline" disabled={busy} onClick={onAdd}>
            {busy && <Loader2 className="spin" size={15} />}
            {busy ? "正在打开..." : "添加字幕目录"}
          </button>
          <button className="ghost" disabled={busy || dirs.length === 0} onClick={onClear}>
            清空
          </button>
        </div>
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

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="info-row">
      <span>{label}</span>
      <strong title={value}>{value}</strong>
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
  const parseEvidence = buildParseEvidence(item);

  return (
    <aside className={`detail-drawer ${props.isOpen ? "open" : ""}`} aria-hidden={!props.isOpen}>
      <div className="drawer-title">
        <button className="round-icon" onClick={props.onClose} aria-label="收起详情">
          <ChevronRight size={18} />
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

          {item.video && (parseEvidence.notes.length > 0 || parseEvidence.candidates.length > 0) && (
            <DrawerPanel
              key={`parse-evidence-${parseEvidence.ownerKey}`}
              title={`解析证据 · ${parseStatusLabel[item.video.parseStatus]}`}
              icon="status_help_question.svg"
              danger={item.video.parseStatus === "ambiguous" || item.video.parseStatus === "lowConfidence"}
            >
              <div className="parse-note-list">
                {parseEvidence.notes.map((note) => (
                  <p className="conflict-note" key={note.id}>
                    {note.text}
                  </p>
                ))}
              </div>
              {parseEvidence.candidates.length > 0 && (
                <div className="candidate-list">
                  {parseEvidence.candidates.map((candidate) => (
                    <div className="candidate-row" key={candidate.id}>
                      <div className="candidate-main">
                        <span className="mini-chip">{candidate.episodeKey}</span>
                        <span className="mini-chip">{candidate.confidence}</span>
                        <span className="recommend">{candidate.sourceLabel}</span>
                      </div>
                      <span>{candidate.note}</span>
                    </div>
                  ))}
                </div>
              )}
            </DrawerPanel>
          )}

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

function buildParseEvidence(item: EpisodeMatch | null) {
  const video = item?.video ?? null;
  if (!item || !video) {
    return { ownerKey: "empty", notes: [], candidates: [] };
  }

  const ownerKey = video.path || item.episodeKey;
  const currentEpisodeKey = video.episodeKey ?? item.episodeKey;
  const shouldShowOnlyCurrentEpisode = video.parseStatus !== "ambiguous";
  const seenNotes = new Set<string>();
  const notes = video.parseNotes.flatMap((note, index) => {
    if (seenNotes.has(note)) {
      return [];
    }
    seenNotes.add(note);
    return [{ id: `${ownerKey}:note:${index}:${note}`, text: note }];
  });

  const seenCandidates = new Set<string>();
  const candidates = video.parseCandidates.flatMap((candidate, index) => {
    if (shouldShowOnlyCurrentEpisode && currentEpisodeKey && candidate.episodeKey !== currentEpisodeKey) {
      return [];
    }

    const signature = `${candidate.episodeKey}\u0000${candidate.confidence}\u0000${candidate.source}\u0000${candidate.note}`;
    if (seenCandidates.has(signature)) {
      return [];
    }
    seenCandidates.add(signature);

    return [
      {
        id: `${ownerKey}:candidate:${index}:${signature}`,
        episodeKey: candidate.episodeKey,
        confidence: candidate.confidence,
        sourceLabel: parseCandidateSourceLabel(candidate.source),
        note: candidate.note,
      },
    ];
  });

  return { ownerKey, notes, candidates };
}

function parseCandidateSourceLabel(source: string) {
  if (source === "template") {
    return "模板";
  }
  if (source === "crf") {
    return "CRF";
  }
  return "规则";
}

function organizedPathFor(
  items: OrganizeExecutionResult["items"],
  episodeKey: string,
  kind: "video" | "subtitle",
  role: "primary" | "secondary" | null,
) {
  return (
    items.find((item) => {
      if (item.episodeKey !== episodeKey || item.kind !== kind) {
        return false;
      }
      if (item.status === "failed" || item.status === "skipped") {
        return false;
      }
      return role ? item.role === role : true;
    })?.destination ?? null
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
  updateAllCollisions: (action: CollisionAction) => void;
  executePlan: () => void;
  isExecuting: boolean;
  progress: OrganizeProgressEvent | null;
  onMinimize: () => void;
}) {
  const progressPercent = props.progress
    ? Math.round((props.progress.processed / Math.max(props.progress.total, 1)) * 100)
    : 0;
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
          <div className="modal-header-actions">
            {props.plan.summary.conflicts > 0 && (
              <div className="collision-bulk-actions" aria-label="批量设置冲突处理方式">
                <button className="ghost" disabled={props.isExecuting} onClick={() => props.updateAllCollisions("skip")}>
                  全部跳过
                </button>
                <button className="ghost" disabled={props.isExecuting} onClick={() => props.updateAllCollisions("replace")}>
                  全部替换
                </button>
              </div>
            )}
            {props.isExecuting && (
              <button className="ghost" onClick={props.onMinimize}>
                <Minimize2 size={15} />
                缩小
              </button>
            )}
            <button className="ghost" disabled={props.isExecuting} onClick={() => props.setPlan(null)}>
              关闭
            </button>
          </div>
        </div>
        {props.isExecuting && (
          <div className="organize-progress-panel" aria-live="polite">
            <div className="progress-heading">
              <strong>正在整理文件</strong>
              <span>
                {props.progress?.processed ?? 0}/{props.progress?.total ?? props.plan.items.length}
              </span>
            </div>
            <div className="progress-track" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={progressPercent}>
              <span style={{ width: `${progressPercent}%` }} />
            </div>
            <p>{props.progress?.message ?? "正在准备文件操作。"}</p>
            {props.progress?.currentDestination && <small title={props.progress.currentDestination}>{props.progress.currentDestination}</small>}
          </div>
        )}
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
                  disabled={props.isExecuting}
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
          <button className="secondary" disabled={props.isExecuting} onClick={() => props.setPlan(null)}>
            取消
          </button>
          <button className="primary" disabled={props.isExecuting} onClick={props.executePlan}>
            {props.isExecuting ? "执行中..." : "确认执行"}
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
  const episodeKey = `${season}E01`;
  return `${projectName} ${season}/
├─ videos/
│  └─ ${projectName} ${episodeKey}.mkv
├─ subs/
│  ├─ zh-Hans/
│  │  └─ ${projectName} ${episodeKey}.zh-Hans.ass
│  └─ ja/
│     └─ ${projectName} ${episodeKey}.ja.srt
└─ anime-sub-map.json（自动生成）`;
}

function projectOutputDir(outputDir: string, projectName: string, season: string) {
  const folderName = `${projectName} ${season}`;
  const normalized = outputDir.replace(/[\\/]+$/, "");
  const tail = normalized.split(/[\\/]/).pop();
  if (tail?.toLocaleLowerCase() === folderName.toLocaleLowerCase()) {
    return normalized;
  }
  return `${normalized}\\${folderName}`;
}

function inferProjectNameFromScanResult(result: ScanAndMatchResult) {
  const counts = new Map<string, number>();
  for (const video of result.scan.videos) {
    const inferred = inferProjectNameFromVideoFileName(video.fileName);
    if (!inferred) {
      continue;
    }
    counts.set(inferred, (counts.get(inferred) ?? 0) + 1);
  }

  return [...counts.entries()].sort((left, right) => right[1] - left[1] || right[0].length - left[0].length)[0]?.[0] ?? null;
}

function inferProjectNameFromVideoDir(path: string) {
  const parts = path.replace(/[\\/]+$/, "").split(/[\\/]/).filter(Boolean);
  const tail = parts[parts.length - 1] ?? "Anime";
  const parent = parts.length > 1 ? parts[parts.length - 2] : undefined;
  if (parent && /^(video|videos|raw|raws|source|sources)$/i.test(tail)) {
    return cleanProjectNameCandidate(parent) || parent;
  }
  return cleanProjectNameCandidate(tail) || tail;
}

function inferProjectNameFromVideoFileName(fileName: string) {
  const withoutExtension = fileName.replace(/\.[^.]+$/, "");
  const withoutLeadingGroups = withoutExtension.replace(/^(?:\[[^\]]+\]\s*)+/, "");
  const readable = withoutLeadingGroups.replace(/[._]+/g, " ");
  const markers = [
    /\bS\d{1,2}E\d{1,3}\b/i,
    /\bS\d{1,2}\b/i,
    /\s-\s*\d{1,3}(?:v\d+)?\b/i,
    /\bEP?\s*\d{1,3}(?:v\d+)?\b/i,
    /第\s*\d{1,3}\s*[话話]/,
  ];
  const markerIndexes = markers.map((marker) => readable.search(marker)).filter((index) => index > 0);
  const candidate = markerIndexes.length > 0 ? readable.slice(0, Math.min(...markerIndexes)) : readable;
  return cleanProjectNameCandidate(candidate);
}

function cleanProjectNameCandidate(value: string) {
  return value
    .replace(/\[[^\]]+\]/g, " ")
    .replace(/\([^)]*(?:720p|1080p|2160p|x264|x265|h\.?264|h\.?265|hevc|web-dl|bdrip)[^)]*\)/gi, " ")
    .replace(/\b(?:720p|1080p|2160p|x264|x265|h\.?264|h\.?265|hevc|web-dl|bdrip|dual|ddp?\d?(?:\.\d)?)\b.*$/i, "")
    .replace(/[-_\s.]+$/g, "")
    .replace(/[-_.]+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
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
    parseStatus: "accepted",
    parseNotes: [`已接受 ${key}，置信度 100。`],
    parseCandidates: [
      {
        episode: { season: 1, episode },
        episodeKey: key,
        confidence: 100,
        source: "rule",
        note: "演示数据：命中单文件强规则。",
      },
    ],
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
    parseStatus: "accepted",
    parseNotes: [`已接受 ${key}，置信度 98。`],
    parseCandidates: [
      {
        episode: { season: 1, episode },
        episodeKey: key,
        confidence: 98,
        source: "rule",
        note: "演示数据：命中单文件强规则。",
      },
    ],
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
  const projectRoot = projectOutputDir(outputDir, projectName, season);
  const items = matches.flatMap((item) => {
    const videoItem = item.video
      ? [
          {
            source: item.video.path,
            destination: `${projectRoot}\\videos\\${projectName} ${item.episodeKey}.${item.video.extension}`,
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
      destination: `${projectRoot}\\subs\\${candidate.language}\\${projectName} ${item.episodeKey}.${candidate.language}.${candidate.extension}`,
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
    outputDir: projectRoot,
    mode,
    items,
    hasConflicts: false,
    mapFilePath: `${projectRoot}\\anime-sub-map.json`,
    mapFileExists: false,
    summary: {
      videos: matches.filter((item) => item.video).length,
      subtitles: items.filter((item) => item.kind === "subtitle").length,
      conflicts: 0,
    },
    projectMap: {},
  };
}

function makePreviewLibraryEntry(request: SaveLocalLibraryRequest): LocalAnimeLibraryEntry {
  const now = Math.floor(Date.now() / 1000);
  return {
    id: `${request.projectName}-${request.season}-${request.outputDir}`.toLowerCase().replace(/[^a-z0-9]+/g, "-"),
    projectName: request.projectName,
    season: request.season,
    outputDir: request.outputDir,
    mode: request.mode,
    episodeCount: request.episodes.length,
    subtitlePreferenceSnapshot: request.subtitlePreferenceSnapshot,
    coverStrategySnapshot: request.coverStrategySnapshot,
    episodes: request.episodes,
    createdAtUnix: now,
    updatedAtUnix: now,
    organizedAtUnix: now,
  };
}
