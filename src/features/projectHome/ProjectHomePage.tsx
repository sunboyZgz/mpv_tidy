import { Check, ChevronLeft, ChevronRight, Copy, Edit3, FolderOpen, Info, Loader2, RefreshCcw, Rocket, Save } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  buildOrganizePlan,
  executeOrganizePlan,
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
} from "../../types";

type ScanState = "idle" | "scanning" | "ready";
type DrawerMode = "closed" | "episodeDetail";

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

export function ProjectHomePage({
  showToast,
  onLibraryEntrySaved,
}: {
  showToast: (message: string) => void;
  onLibraryEntrySaved: (entry: LocalAnimeLibraryEntry) => void;
}) {
  const [projectName, setProjectName] = useState("Jujutsu Kaisen");
  const [season, setSeason] = useState("S01");
  const [videoDir, setVideoDir] = useState<string | null>(null);
  const [subtitleDirs, setSubtitleDirs] = useState<string[]>([]);
  const [outputDir, setOutputDir] = useState<string | null>(null);
  const [outputHistory, setOutputHistory] = useState<string[]>([]);
  const [scanState, setScanState] = useState<ScanState>("idle");
  const [scanResult, setScanResult] = useState<ScanAndMatchResult | null>(null);
  const [selectedEpisodeKey, setSelectedEpisodeKey] = useState<string | null>(null);
  const { drawer, openDetailDrawer, closeDetailDrawer } = useProjectHomeWorkflow();
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
    const selected = await selectDirectory();
    if (selected) {
      setVideoDir(selected);
      setScanResult(null);
      setScanState("idle");
      setSelectedEpisodeKey(null);
      closeDetailDrawer(null);
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
    const paths = await selectDirectories();
    if (paths.length === 0) {
      return;
    }
    setSubtitleDirs((current) => unique([...current, ...paths]));
    setScanResult(null);
    setScanState("idle");
    closeDetailDrawer(null);
  }

  async function chooseOutputDir() {
    if (!isTauriRuntime()) {
      const sample = "D:\\整理输出\\Jujutsu Kaisen S01";
      setOutputDir(sample);
      setOutputHistory((current) => unique([sample, ...current]).slice(0, 3));
      setMessage(browserPreviewMessage);
      return;
    }
    const selected = await selectDirectory();
    if (selected) {
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
      const result = await buildOrganizePlan({
        projectName,
        season,
        outputDir,
        matches,
        mode: organizeMode,
        primaryLanguage: "zh-Hans",
        secondaryLanguage: "ja",
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
      const result = await executeOrganizePlan(plan);
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
        <PlanModal plan={plan} setPlan={setPlan} updateCollision={updateCollision} executePlan={executePlan} />
      )}
    </main>
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

          {item.video && item.video.parseNotes.length > 0 && (
            <DrawerPanel
              title={`解析证据 · ${parseStatusLabel[item.video.parseStatus]}`}
              icon="status_help_question.svg"
              danger={item.video.parseStatus === "ambiguous" || item.video.parseStatus === "lowConfidence"}
            >
              <div className="parse-note-list">
                {item.video.parseNotes.map((note) => (
                  <p className="conflict-note" key={note}>
                    {note}
                  </p>
                ))}
              </div>
              {item.video.parseCandidates.length > 0 && (
                <div className="candidate-list">
                  {item.video.parseCandidates.map((candidate) => (
                    <div className="candidate-row" key={`${candidate.episodeKey}-${candidate.source}`}>
                      <div className="candidate-main">
                        <span className="mini-chip">{candidate.episodeKey}</span>
                        <span className="mini-chip">{candidate.confidence}</span>
                        <span className="recommend">{candidate.source === "template" ? "模板" : "规则"}</span>
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

function makePreviewLibraryEntry(request: SaveLocalLibraryRequest): LocalAnimeLibraryEntry {
  return {
    projectName: request.projectName,
    season: request.season,
    outputDir: request.outputDir,
    mode: request.mode,
    episodeCount: request.episodes.length,
    episodes: request.episodes,
    organizedAtUnix: Math.floor(Date.now() / 1000),
  };
}
