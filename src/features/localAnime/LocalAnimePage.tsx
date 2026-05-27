import { ChevronLeft, ChevronRight, Edit3, FolderOpen, Minus, Play, Plus, RefreshCcw, RotateCcw, Search, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  launchMpv,
  loadAppSettings,
  loadLocalLibrary,
  removeLocalLibraryEntry,
  revealPath as revealPathCommand,
  updateLibraryEpisodeProgress,
} from "../../services/tauriCommands";
import {
  asset,
  fileNameFromPath,
  formatBytes,
  formatDuration,
  isTauriRuntime,
  splitTextList,
  unique,
} from "../../shared/utils";
import type { LanguageCode, LibraryEpisodeRecord, LocalAnimeLibraryEntry } from "../../types";
import type { AppSettings, WatchStatus } from "../../types";
import "./localAnime.css";

type SubtitleFormat = "ass" | "srt" | "ssa" | "vtt" | "unknown";
type DrawerMode = "closed" | "playback";

interface DrawerState {
  mode: DrawerMode;
  episodeId: string | null;
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

interface LocalAnimePageState {
  searchQuery: string;
  selectedAnimeId?: string;
  selectedEpisodeId?: string;
  drawer: DrawerState;
  primarySubtitleId?: string;
  secondarySubtitleId?: string;
  primaryOffset: number;
  secondaryOffset: number;
  rememberOffset: boolean;
  isLaunching: boolean;
}

const watchStatusLabel: Record<WatchStatus, string> = {
  watched: "已观看",
  partial: "部分观看",
  unwatched: "未观看",
};

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

export function useLocalAnimePlayback() {
  const [drawer, setDrawer] = useState<DrawerState>({ mode: "playback", episodeId: null });

  function openPlaybackDrawer(episodeId: string | null) {
    setDrawer({ mode: episodeId ? "playback" : "closed", episodeId });
  }

  function closePlaybackDrawer(episodeId: string | null) {
    setDrawer({ mode: "closed", episodeId });
  }

  return { drawer, openPlaybackDrawer, closePlaybackDrawer };
}

export function LocalAnimePage({
  showToast,
  syncedEntry,
}: {
  showToast: (message: string) => void;
  syncedEntry: LocalAnimeLibraryEntry | null;
}) {
  const [library, setLibrary] = useState<LocalAnimeEntryUi[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedAnimeId, setSelectedAnimeId] = useState<string | undefined>();
  const [selectedEpisodeId, setSelectedEpisodeId] = useState<string | undefined>();
  const { drawer, openPlaybackDrawer, closePlaybackDrawer } = useLocalAnimePlayback();
  const [primarySubtitleId, setPrimarySubtitleId] = useState<string | undefined>();
  const [secondarySubtitleId, setSecondarySubtitleId] = useState<string | undefined>();
  const [primaryOffset, setPrimaryOffset] = useState(0);
  const [secondaryOffset, setSecondaryOffset] = useState(0);
  const [rememberOffset, setRememberOffset] = useState(true);
  const [offsetPrefs, setOffsetPrefs] = useState<Record<string, DirectorySubtitleOffsetPreference>>({});
  const [isLaunching, setIsLaunching] = useState(false);
  const [appSettings, setAppSettings] = useState<AppSettings>(fallbackAppSettings);
  const [panelMessage, setPanelMessage] = useState("选择剧集后可调整字幕并启动 MPV。");
  const [editingAnime, setEditingAnime] = useState<LocalAnimeEntryUi | null>(null);
  const launchInFlightRef = useRef(false);

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
  const pageState: LocalAnimePageState = {
    searchQuery,
    selectedAnimeId,
    selectedEpisodeId,
    drawer,
    primarySubtitleId,
    secondarySubtitleId,
    primaryOffset,
    secondaryOffset,
    rememberOffset,
    isLaunching,
  };
  const playbackDrawerOpen = pageState.drawer.mode === "playback";

  const summary = useMemo(
    () => ({
      animeCount: library.length,
      episodeCount: library.reduce((sum, entry) => sum + entry.episodes.length, 0),
    }),
    [library],
  );

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }
    loadAppSettings()
      .then((settings) => {
        setAppSettings(settings);
        if (settings.autoScanAnimeLibraryOnStartup) {
          void refreshLibrary();
        } else {
          setPanelMessage("已按设置跳过启动时刷新，可手动刷新本地动漫库。");
        }
      })
      .catch((error) => {
        setPanelMessage(`设置读取失败：${String(error)}`);
        void refreshLibrary();
      });
  }, []);

  useEffect(() => {
    if (!syncedEntry) {
      return;
    }
    const nextEntry = libraryEntryToUi(syncedEntry);
    setLibrary((current) => upsertAnimeEntry(current, nextEntry));
    setSelectedAnimeId(nextEntry.id);
    showToast("本地动漫库已更新");
  }, [syncedEntry]);

  useEffect(() => {
    if (library.length === 0) {
      setSelectedAnimeId(undefined);
      setSelectedEpisodeId(undefined);
      closePlaybackDrawer(null);
      return;
    }
    if (!selectedAnimeId || !library.some((entry) => entry.id === selectedAnimeId)) {
      setSelectedAnimeId(selectDefaultAnimeId(library));
    }
  }, [library, selectedAnimeId]);

  useEffect(() => {
    if (!selectedAnime) {
      closePlaybackDrawer(null);
      return;
    }
    const lastEpisode =
      selectedAnime.episodes.find((episode) => episode.id === selectedAnime.lastWatchedEpisodeId) ??
      selectedAnime.episodes[0];
    setSelectedEpisodeId(lastEpisode?.id);
    openPlaybackDrawer(lastEpisode?.id ?? null);

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
      selectedEpisode.subtitles.find((subtitle) => subtitle.language === appSettings.defaultPrimarySubtitleLanguage) ??
      selectedEpisode.subtitles.find((subtitle) => subtitle.role === "primary") ??
      selectedEpisode.subtitles[0];
    const secondary =
      selectedEpisode.subtitles.find(
        (subtitle) => subtitle.language === appSettings.defaultSecondarySubtitleLanguage && subtitle.id !== primary?.id,
      ) ??
      selectedEpisode.subtitles.find((subtitle) => subtitle.role === "secondary" && subtitle.id !== primary?.id) ??
      selectedEpisode.subtitles.find((subtitle) => subtitle.id !== primary?.id);
    setPrimarySubtitleId(primary?.id);
    setSecondarySubtitleId(secondary?.id);
  }, [appSettings.defaultPrimarySubtitleLanguage, appSettings.defaultSecondarySubtitleLanguage, selectedEpisode]);

  async function refreshLibrary() {
    if (!isTauriRuntime()) {
      setPanelMessage("浏览器预览中暂无本地库文件；请从项目首页保存结果后查看。");
      showToast("本地动漫库已刷新");
      return;
    }
    try {
      const loaded = await loadLocalLibrary();
      const entries = loaded.entries.map(libraryEntryToUi);
      setLibrary(entries);
      setPanelMessage(entries.length === 0 ? "本地动漫库为空。" : "本地动漫库已刷新。");
      showToast("本地动漫库已刷新");
    } catch (error) {
      setPanelMessage(String(error));
    }
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
    if (launchInFlightRef.current) {
      setPanelMessage("MPV 正在处理上一次播放请求。");
      return;
    }
    if (!selectedEpisode) {
      setPanelMessage("请先选择一个剧集。");
      return;
    }
    if (!selectedEpisode.videoPath) {
      setPanelMessage("当前剧集缺少视频路径，无法启动 MPV。");
      return;
    }
    if (duplicateSubtitleSelected) {
      setPanelMessage("主字幕和副字幕不能选择同一个文件。");
      return;
    }

    launchInFlightRef.current = true;
    setIsLaunching(true);
    const extraArgs: string[] = [];
    extraArgs.push(appSettings.rememberPlaybackProgress ? "--save-position-on-quit" : "--no-resume-playback");

    if (!isTauriRuntime()) {
      window.setTimeout(() => {
        setIsLaunching(false);
        launchInFlightRef.current = false;
        markEpisodeWatched(selectedEpisode.id, "partial");
        showToast("MPV 已启动");
      }, 320);
      return;
    }

    try {
      const result = await launchMpv({
        mpvPath: appSettings.mpvExecutablePath,
        videoPath: selectedEpisode.videoPath,
        primarySubtitle: selectedPrimarySubtitle?.path ?? null,
        secondarySubtitle: selectedSecondarySubtitle?.path ?? null,
        primarySubtitleDelaySeconds: primaryOffset,
        secondarySubtitleDelaySeconds: secondaryOffset,
        extraArgs,
      });
      markEpisodeWatched(selectedEpisode.id, "partial");
      if (appSettings.autoSaveWatchProgress) {
        await persistEpisodeProgress(selectedEpisode, "partial");
      }
      showToast(result.switchedVideo ? `已切换到 ${selectedEpisode.episodeKey}` : "MPV 已启动");
    } catch (error) {
      setPanelMessage(String(error));
    } finally {
      setIsLaunching(false);
      launchInFlightRef.current = false;
    }
  }

  async function persistEpisodeProgress(episode: LocalEpisode, status: WatchStatus) {
    if (!selectedAnime || !isTauriRuntime()) {
      return;
    }
    const updated = await updateLibraryEpisodeProgress({
      entryId: selectedAnime.id,
      episodeKey: episode.episodeKey,
      watchStatus: status,
      lastPositionSec: episode.lastPositionSec ?? null,
      progressPercent: Math.max(episode.progressPercent ?? 0, 12),
    });
    setLibrary((current) => upsertAnimeEntry(current, libraryEntryToUi(updated)));
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
      await revealPathCommand(path);
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

  async function removeAnimeFromLibrary(entry: LocalAnimeEntryUi) {
    const confirmed = window.confirm(`从本地动漫库移除「${entry.title}」？\n\n只会移除库记录，不会删除本地视频或字幕文件。`);
    if (!confirmed) {
      return;
    }

    if (!isTauriRuntime()) {
      setLibrary((current) => current.filter((candidate) => candidate.id !== entry.id));
      setSelectedAnimeId(undefined);
      setSelectedEpisodeId(undefined);
      closePlaybackDrawer(null);
      showToast("已从本地动漫库移除");
      return;
    }

    try {
      const updated = await removeLocalLibraryEntry({ entryId: entry.id });
      const entries = updated.entries.map(libraryEntryToUi);
      setLibrary(entries);
      setSelectedAnimeId(selectDefaultAnimeId(entries));
      setSelectedEpisodeId(undefined);
      closePlaybackDrawer(null);
      showToast("已从本地动漫库移除");
    } catch (error) {
      setPanelMessage(String(error));
    }
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
              <AnimeDetailCard
                entry={selectedAnime}
                onEdit={() => selectedAnime && setEditingAnime(selectedAnime)}
                onRemove={() => selectedAnime && void removeAnimeFromLibrary(selectedAnime)}
              />
              <EpisodeTable
                anime={selectedAnime}
                selectedEpisodeId={selectedEpisode?.id}
                onSelect={(episode) => {
                  setSelectedEpisodeId(episode.id);
                  openPlaybackDrawer(episode.id);
                }}
                onDoubleClick={() => {
                  void playWithMpv();
                }}
              />
            </section>
          </div>
        </div>
      </section>

      <PlaybackPanel
        anime={selectedAnime}
        episode={selectedEpisode}
        isOpen={playbackDrawerOpen}
        onClose={() => closePlaybackDrawer(selectedEpisode?.id ?? null)}
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
      {!playbackDrawerOpen && selectedEpisode && (
        <button
          className="drawer-expand-tab library-drawer-expand-tab"
          aria-label="展开播放设置"
          onClick={() => openPlaybackDrawer(selectedEpisode.id)}
        >
          <ChevronLeft size={18} />
        </button>
      )}

      {editingAnime && (
        <AnimeEditModal anime={editingAnime} onCancel={() => setEditingAnime(null)} onSave={saveAnimeInfo} />
      )}
    </main>
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

function AnimeDetailCard({
  entry,
  onEdit,
  onRemove,
}: {
  entry: LocalAnimeEntryUi | null;
  onEdit: () => void;
  onRemove: () => void;
}) {
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
        <div className="anime-detail-actions">
          <button className="edit-info-button" onClick={onEdit}>
            <Edit3 size={17} />
            编辑信息
          </button>
          <button className="remove-library-button" onClick={onRemove}>
            <Trash2 size={17} />
            移出库
          </button>
        </div>
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
  isOpen: boolean;
  onClose: () => void;
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
    <aside
      className={`playback-panel ${props.isOpen ? "open" : ""}`}
      aria-hidden={!props.isOpen}
      data-testid="playback-panel"
    >
      <div className="playback-title">
        <button className="round-icon" aria-label="收起播放设置" onClick={props.onClose}>
          <ChevronRight size={18} />
        </button>
        <h2>{props.episode?.episodeKey ?? "--"} {props.episode?.title ?? "未选择剧集"}</h2>
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
        disabled={!props.episode?.videoPath || props.isLaunching || props.duplicateSubtitleSelected}
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
        aria-label={props.label}
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
      <button aria-label={`${props.label}减少`} onClick={() => props.onChange(roundOffset(props.value - 0.1))}>
        <Minus size={16} />
      </button>
      <input
        aria-label={props.label}
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
      <button aria-label={`${props.label}增加`} onClick={() => props.onChange(roundOffset(props.value + 0.1))}>
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

function formatPlaybackProgress(episode: LocalEpisode | null) {
  if (!episode?.lastPositionSec) {
    return "未开始";
  }
  return `${formatDuration(episode.lastPositionSec)} (${episode.progressPercent ?? 0}%)`;
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

function selectDefaultAnimeId(entries: LocalAnimeEntryUi[]) {
  return entries.find((entry) => entry.lastWatchedEpisodeId)?.id ?? entries[0]?.id;
}

function upsertAnimeEntry(entries: LocalAnimeEntryUi[], nextEntry: LocalAnimeEntryUi) {
  const existingIndex = entries.findIndex((entry) => entry.id === nextEntry.id);
  if (existingIndex < 0) {
    return [nextEntry, ...entries];
  }
  return entries.map((entry, index) => (index === existingIndex ? nextEntry : entry));
}

function libraryEntryToUi(entry: LocalAnimeLibraryEntry): LocalAnimeEntryUi {
  const id = libraryEntryId(entry);
  const episodes = entry.episodes.map((episode) => libraryEpisodeToUi(episode, id));
  const subtitleDirs = unique(
    episodes.flatMap((episode) => episode.subtitles.map((subtitle) => pathDirectory(subtitle.path))).filter(Boolean),
  );
  return {
    id,
    title: `${entry.projectName} ${entry.season}`.trim(),
    alias: [entry.projectName],
    type: entry.episodeCount === 1 ? "Movie" : "TV 动画",
    tags: [entry.mode === "copy" ? "复制整理" : "移动整理"],
    description: "从项目首页整理并保存到本地动漫库。",
    coverTone: coverToneFor(entry.projectName),
    rootDir: entry.outputDir,
    videoDir: pathDirectory(episodes.find((episode) => episode.videoPath)?.videoPath) || `${entry.outputDir}\\videos`,
    subtitleDirs,
    subtitleLanguages: unique(episodes.flatMap((episode) => episode.subtitles.map((subtitle) => subtitle.language))),
    episodes,
    lastWatchedEpisodeId: episodes.find((episode) => episode.watchStatus !== "unwatched")?.id,
    createdAt: dateFromUnix(entry.createdAtUnix || entry.organizedAtUnix),
    updatedAt: dateFromUnix(entry.updatedAtUnix || entry.organizedAtUnix),
  };
}

function libraryEpisodeToUi(record: LibraryEpisodeRecord, entryId: string): LocalEpisode {
  const subtitles = [
    subtitleFromPath(record.primarySubtitlePath, `${entryId}-${record.episodeKey}-primary`, "primary"),
    subtitleFromPath(record.secondarySubtitlePath, `${entryId}-${record.episodeKey}-secondary`, "secondary"),
  ].filter((subtitle): subtitle is LocalSubtitle => Boolean(subtitle));
  const dedupedSubtitles = subtitles.filter(
    (subtitle, index) => subtitles.findIndex((candidate) => candidate.path === subtitle.path) === index,
  );
  return {
    id: `${entryId}-${record.episodeKey}`,
    episodeKey: record.episodeKey,
    title: record.episodeKey,
    videoPath: record.videoPath ?? "",
    subtitles: dedupedSubtitles,
    watchStatus: record.watchStatus ?? "unwatched",
    lastPositionSec: record.lastPositionSec ?? undefined,
    progressPercent: record.progressPercent ?? 0,
  };
}

function subtitleFromPath(path: string | null, id: string, role: LocalSubtitle["role"]): LocalSubtitle | null {
  if (!path) {
    return null;
  }
  return {
    id,
    path,
    language: detectLanguageFromPath(path),
    format: subtitleFormatFromPath(path),
    role,
  };
}

function libraryEntryId(entry: LocalAnimeLibraryEntry) {
  if (entry.id) {
    return entry.id;
  }
  const raw = `${entry.projectName}-${entry.season}-${entry.outputDir}`;
  const normalized = raw.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  return normalized || `library-${entry.organizedAtUnix}`;
}

function detectLanguageFromPath(path: string): LanguageCode {
  const lower = path.toLowerCase();
  if (lower.includes("zh-hans") || lower.includes("zh-cn") || lower.includes("zh_cn") || lower.includes("chs") || lower.includes("简中") || lower.includes("简体")) {
    return "zh-Hans";
  }
  if (lower.includes("zh-hant") || lower.includes("zh-tw") || lower.includes("zh_tw") || lower.includes("cht") || lower.includes("繁中") || lower.includes("繁体")) {
    return "zh-Hant";
  }
  if (lower.includes("ja-jp") || lower.includes("jpn") || lower.includes("ja") || lower.includes("日文") || lower.includes("日本語")) {
    return "ja";
  }
  if (lower.includes("english") || lower.includes("eng") || lower.includes("\\en\\") || lower.includes("/en/") || lower.endsWith(".en.srt") || lower.endsWith(".en.ass")) {
    return "en";
  }
  return "und";
}

function subtitleFormatFromPath(path: string): SubtitleFormat {
  const extension = pathExtension(path);
  if (extension === "ass" || extension === "srt" || extension === "ssa" || extension === "vtt") {
    return extension;
  }
  return "unknown";
}

function pathDirectory(path: string | undefined) {
  if (!path) {
    return "";
  }
  const normalized = path.replace(/[\\/]+$/, "");
  const slash = Math.max(normalized.lastIndexOf("\\"), normalized.lastIndexOf("/"));
  return slash >= 0 ? normalized.slice(0, slash) : "";
}

function pathExtension(path: string) {
  const fileName = fileNameFromPath(path);
  const dot = fileName.lastIndexOf(".");
  return dot >= 0 ? fileName.slice(dot + 1).toLowerCase() : "";
}

function dateFromUnix(value: number) {
  return new Date(value * 1000).toISOString();
}

function coverToneFor(value: string) {
  const tones = ["violet", "blue", "red", "amber", "green", "pink", "sky"];
  const score = Array.from(value).reduce((sum, char) => sum + char.charCodeAt(0), 0);
  return tones[score % tones.length];
}
