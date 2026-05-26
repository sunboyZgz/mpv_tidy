import {
  Captions,
  FolderOpen,
  HardDrive,
  Info,
  Library,
  PlayCircle,
  RotateCcw,
  Save,
} from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  loadAppSettings,
  loadSettingsStoragePaths,
  resetAppSettings,
  saveAppSettings,
  selectDirectory,
  selectFile,
} from "../../services/tauriCommands";
import { asset, browserPreviewMessage, chipClass, isTauriRuntime } from "../../shared/utils";
import type { AppSettings, CoverStrategy } from "../../types";
import "./settings.css";

type SubtitleLanguage = "zh-Hans" | "zh-Hant" | "en" | "ja";
type PathSettingType = "mpvExecutablePath" | "defaultOutputDir" | "animeLibraryRootDir" | "tempDir";
const previewTrainingDataDir = "C:\\Users\\User\\AppData\\Roaming\\com.mpvtidy.animesubtitlemanager";

type SettingsState = AppSettings & {
  trainingDataDir: string;
};

const defaultSettings: SettingsState = {
  schemaVersion: 1,
  mpvExecutablePath: "mpv",
  defaultOutputDir: "D:\\整理输出",
  animeLibraryRootDir: "D:\\AnimeLibrary",
  tempDir: "C:\\Users\\User\\AppData\\Local\\mpv_tidy\\temp",
  trainingDataDir: previewTrainingDataDir,
  defaultPrimarySubtitleLanguage: "zh-Hans",
  defaultSecondarySubtitleLanguage: "en",
  rememberPlaybackProgress: true,
  autoScanAnimeLibraryOnStartup: true,
  autoSaveWatchProgress: true,
  defaultCoverStrategy: "local-first-then-screenshot",
  updatedAtUnix: 0,
};

const languageOptions: Array<{ value: SubtitleLanguage; label: string; summary: string }> = [
  { value: "zh-Hans", label: "zh-Hans（简体中文）", summary: "简体中文" },
  { value: "zh-Hant", label: "zh-Hant（繁体中文）", summary: "繁体中文" },
  { value: "en", label: "English（英文）", summary: "英文" },
  { value: "ja", label: "Japanese（日文）", summary: "日文" },
];

const coverStrategyOptions: Array<{ value: CoverStrategy; label: string; summary: string }> = [
  {
    value: "local-first-then-screenshot",
    label: "优先使用本地封面，缺失时使用视频截图",
    summary: "本地封面优先，缺失时截图",
  },
  { value: "local-only", label: "仅使用本地封面", summary: "仅本地封面" },
  { value: "screenshot-only", label: "仅使用视频截图", summary: "仅视频截图" },
  { value: "disabled", label: "不自动获取封面", summary: "手动设置封面" },
];

function loadSettings(): SettingsState {
  return defaultSettings;
}

async function saveSettings(settings: SettingsState) {
  if (!isTauriRuntime()) {
    return settings;
  }
  const saved = await saveAppSettings(toAppSettings(settings));
  return withTrainingDataDir(saved, settings.trainingDataDir);
}

function resetSettingsToDefault(trainingDataDir = defaultSettings.trainingDataDir): SettingsState {
  return { ...defaultSettings, trainingDataDir };
}

function toAppSettings(settings: SettingsState): AppSettings {
  const { trainingDataDir: _trainingDataDir, ...appSettings } = settings;
  return appSettings;
}

function withTrainingDataDir(settings: AppSettings, trainingDataDir: string): SettingsState {
  return { ...settings, trainingDataDir };
}

export function SettingsPage({ showToast }: { showToast: (message: string) => void }) {
  const [savedSettings, setSavedSettings] = useState<SettingsState>(() => loadSettings());
  const [settings, setSettings] = useState<SettingsState>(savedSettings);

  const coverStrategyLabel = useMemo(
    () => coverStrategyOptions.find((option) => option.value === settings.defaultCoverStrategy)?.label ?? "",
    [settings.defaultCoverStrategy],
  );
  const hasUnsavedChanges = useMemo(
    () => JSON.stringify(settings) !== JSON.stringify(savedSettings),
    [savedSettings, settings],
  );

  function updateSetting<K extends keyof SettingsState>(key: K, value: SettingsState[K]) {
    setSettings((current) => ({ ...current, [key]: value }));
  }

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let mounted = true;
    Promise.all([loadAppSettings(), loadSettingsStoragePaths()])
      .then(([loadedSettings, paths]) => {
        if (!mounted) {
          return;
        }
        const next = withTrainingDataDir(loadedSettings, paths.trainingDataDir);
        setSavedSettings(next);
        setSettings(next);
      })
      .catch((error) => {
        showToast(`设置读取失败：${String(error)}`);
      });

    return () => {
      mounted = false;
    };
  }, []);

  async function handleBrowsePath(type: PathSettingType) {
    if (!isTauriRuntime()) {
      showToast(browserPreviewMessage);
      return;
    }

    const selected = type === "mpvExecutablePath" ? await selectFile() : await selectDirectory();
    if (selected) {
      updateSetting(type, selected);
    }
  }

  async function handleSave() {
    try {
      const saved = await saveSettings(settings);
      setSettings(saved);
      setSavedSettings(saved);
      showToast("设置已保存");
    } catch (error) {
      showToast(`设置保存失败：${String(error)}`);
    }
  }

  function handleCancel() {
    setSettings(savedSettings);
    showToast("已还原为上次保存的设置");
  }

  function handleRestoreDefaults() {
    const confirmed = window.confirm("是否将所有设置恢复为默认值？");
    if (!confirmed) {
      return;
    }
    if (!isTauriRuntime()) {
      setSettings(resetSettingsToDefault(settings.trainingDataDir));
      showToast("已恢复默认设置");
      return;
    }
    resetAppSettings()
      .then((next) => {
        const restored = withTrainingDataDir(next, settings.trainingDataDir);
        setSettings(restored);
        setSavedSettings(restored);
        showToast("已恢复默认设置");
      })
      .catch((error) => {
        showToast(`恢复默认失败：${String(error)}`);
      });
  }

  return (
    <main className="workspace settings-workspace">
      <img className="settings-floating-flower" src={asset("images/detail_flower_trimmed.png")} alt="" />
      <header className="settings-header">
        <div>
          <h1>设置</h1>
          <p>管理播放器、字幕匹配与本地动漫库的默认行为</p>
        </div>
        <div className="settings-actions" aria-label="设置操作">
          <span className={`settings-save-state ${hasUnsavedChanges ? "dirty" : "saved"}`} aria-live="polite">
            {hasUnsavedChanges ? "有未保存更改" : "已保存"}
          </span>
          <button className="settings-button secondary" onClick={handleRestoreDefaults}>
            <RotateCcw size={17} />
            恢复默认
          </button>
          <button className="settings-button outline" onClick={handleCancel}>
            取消
          </button>
          <button className="settings-button primary" onClick={handleSave}>
            <Save size={17} />
            保存设置
          </button>
        </div>
      </header>

      <section className="settings-stage">
        <div className="settings-main">
          <div className="settings-card-grid">
            <BasicPathSettingsCard settings={settings} onBrowse={handleBrowsePath} onUpdate={updateSetting} />
            <SubtitleSettingsCard settings={settings} onUpdate={updateSetting} />
            <MpvPlaybackSettingsCard settings={settings} onUpdate={updateSetting} />
            <AnimeLibrarySettingsCard
              coverStrategyLabel={coverStrategyLabel}
              settings={settings}
              onBrowse={handleBrowsePath}
              onUpdate={updateSetting}
            />
          </div>
          <img className="settings-scenery-art" src={asset("images/setting_bg_trimmed.png")} alt="" />
        </div>
        <SettingsSummaryPanel settings={settings} />
      </section>
    </main>
  );
}

function BasicPathSettingsCard({
  settings,
  onBrowse,
  onUpdate,
}: {
  settings: SettingsState;
  onBrowse: (type: PathSettingType) => void;
  onUpdate: <K extends keyof SettingsState>(key: K, value: SettingsState[K]) => void;
}) {
  return (
    <SettingsCard className="basic-path-card" icon={<FolderOpen size={19} />} title="基础路径设置">
      <PathSettingRow
        id="settings-mpv-path"
        label="MPV 可执行文件路径"
        value={settings.mpvExecutablePath}
        onBrowse={() => onBrowse("mpvExecutablePath")}
        onChange={(value) => onUpdate("mpvExecutablePath", value)}
      />
      <PathSettingRow
        id="settings-output-dir"
        label="默认整理输出目录"
        value={settings.defaultOutputDir}
        onBrowse={() => onBrowse("defaultOutputDir")}
        onChange={(value) => onUpdate("defaultOutputDir", value)}
      />
      <PathSettingRow
        id="settings-library-root-basic"
        label="本地动漫库根目录"
        value={settings.animeLibraryRootDir}
        onBrowse={() => onBrowse("animeLibraryRootDir")}
        onChange={(value) => onUpdate("animeLibraryRootDir", value)}
      />
      <PathSettingRow
        id="settings-temp-dir"
        label="临时文件目录"
        value={settings.tempDir}
        onBrowse={() => onBrowse("tempDir")}
        onChange={(value) => onUpdate("tempDir", value)}
      />
      <PathSettingRow
        id="settings-training-data-dir"
        label="训练数据目录"
        value={settings.trainingDataDir}
        disabled
        readonlyText="自动管理"
      />
      <HelperText>这些路径会用于字幕整理、扫描缓存与播放器调用。</HelperText>
    </SettingsCard>
  );
}

function SubtitleSettingsCard({
  settings,
  onUpdate,
}: {
  settings: SettingsState;
  onUpdate: <K extends keyof SettingsState>(key: K, value: SettingsState[K]) => void;
}) {
  return (
    <SettingsCard icon={<Captions size={19} />} title="字幕匹配设置">
      <label className="settings-field" htmlFor="settings-primary-language">
        <span>默认主字幕语言</span>
        <select
          id="settings-primary-language"
          value={settings.defaultPrimarySubtitleLanguage}
          onChange={(event) =>
            onUpdate("defaultPrimarySubtitleLanguage", event.target.value as SubtitleLanguage)
          }
        >
          {languageOptions.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </label>
      <label className="settings-field" htmlFor="settings-secondary-language">
        <span>默认副字幕语言</span>
        <select
          id="settings-secondary-language"
          value={settings.defaultSecondarySubtitleLanguage}
          onChange={(event) =>
            onUpdate("defaultSecondarySubtitleLanguage", event.target.value as SubtitleLanguage)
          }
        >
          {languageOptions.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </label>
      <div className="settings-language-chips" aria-label="支持的字幕语言">
        {languageOptions.map((option) => (
          <LanguageChip value={option.value} key={option.value} />
        ))}
      </div>
      <HelperText>播放时会优先加载这里配置的主字幕与副字幕语言。</HelperText>
    </SettingsCard>
  );
}

function MpvPlaybackSettingsCard({
  settings,
  onUpdate,
}: {
  settings: SettingsState;
  onUpdate: <K extends keyof SettingsState>(key: K, value: SettingsState[K]) => void;
}) {
  return (
    <SettingsCard icon={<PlayCircle size={19} />} title="MPV 播放设置">
      <ToggleSetting
        checked={settings.rememberPlaybackProgress}
        description="重新打开同一个视频时，可以从上次观看的位置继续播放。"
        id="settings-remember-progress"
        label="记住播放进度"
        onChange={(checked) => onUpdate("rememberPlaybackProgress", checked)}
      />
      <StatusChip enabled={settings.rememberPlaybackProgress} />
    </SettingsCard>
  );
}

function AnimeLibrarySettingsCard({
  coverStrategyLabel,
  settings,
  onBrowse,
  onUpdate,
}: {
  coverStrategyLabel: string;
  settings: SettingsState;
  onBrowse: (type: PathSettingType) => void;
  onUpdate: <K extends keyof SettingsState>(key: K, value: SettingsState[K]) => void;
}) {
  return (
    <SettingsCard icon={<Library size={19} />} title="本地动漫库设置">
      <PathSettingRow
        id="settings-library-root"
        label="本地动漫库根目录"
        value={settings.animeLibraryRootDir}
        onBrowse={() => onBrowse("animeLibraryRootDir")}
        onChange={(value) => onUpdate("animeLibraryRootDir", value)}
      />
      <ToggleSetting
        checked={settings.autoScanAnimeLibraryOnStartup}
        id="settings-auto-scan"
        label="启动时自动扫描本地动漫库"
        onChange={(checked) => onUpdate("autoScanAnimeLibraryOnStartup", checked)}
      />
      <ToggleSetting
        checked={settings.autoSaveWatchProgress}
        id="settings-auto-save-progress"
        label="自动保存观看进度"
        onChange={(checked) => onUpdate("autoSaveWatchProgress", checked)}
      />
      <label className="settings-field" htmlFor="settings-cover-strategy">
        <span>默认封面获取策略</span>
        <select
          id="settings-cover-strategy"
          value={settings.defaultCoverStrategy}
          onChange={(event) => onUpdate("defaultCoverStrategy", event.target.value as CoverStrategy)}
          title={coverStrategyLabel}
        >
          {coverStrategyOptions.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </label>
      <HelperText>封面策略：本地图片 &gt; 文件夹封面 &gt; 视频截图。</HelperText>
    </SettingsCard>
  );
}

function SettingsSummaryPanel({ settings }: { settings: SettingsState }) {
  const primaryLabel = languageLabel(settings.defaultPrimarySubtitleLanguage);
  const secondaryLabel = languageLabel(settings.defaultSecondarySubtitleLanguage);
  const coverSummary =
    coverStrategyOptions.find((option) => option.value === settings.defaultCoverStrategy)?.summary ?? "";

  return (
    <aside className="settings-summary-panel">
      <div className="summary-panel-heading">
        <HardDrive size={19} />
        <h2>设置摘要</h2>
      </div>
      <div className="summary-list">
        <SummaryItem
          label="MPV"
          value={settings.mpvExecutablePath.trim() ? "已配置" : "未配置"}
          tone={settings.mpvExecutablePath.trim() ? "success" : "muted"}
        />
        <SummaryItem label="输出目录" value={settings.defaultOutputDir || "未配置"} />
        <SummaryItem label="字幕偏好" value={`${primaryLabel} / ${secondaryLabel}`} />
        <SummaryItem label="封面策略" value={coverSummary} />
        <SummaryItem
          label="扫描"
          value={settings.autoScanAnimeLibraryOnStartup ? "启动时自动扫描" : "不自动扫描"}
          tone={settings.autoScanAnimeLibraryOnStartup ? "success" : "muted"}
        />
        <SummaryItem
          label="进度"
          value={settings.autoSaveWatchProgress ? "自动保存观看进度" : "不自动保存"}
          tone={settings.autoSaveWatchProgress ? "success" : "muted"}
        />
      </div>
      <div className="settings-tips">
        <div className="settings-tips-title">
          <Info size={16} />
          <h3>提示</h3>
        </div>
        <p>修改设置后请点击“保存设置”。部分配置会在下一次扫描或下一次启动应用时生效。</p>
      </div>
      <img className="summary-flower" src={asset("images/setting_bg2_trimmed.png")} alt="" />
    </aside>
  );
}

function SettingsCard({
  children,
  className,
  icon,
  title,
}: {
  children: ReactNode;
  className?: string;
  icon: ReactNode;
  title: string;
}) {
  return (
    <section className={`settings-card ${className ?? ""}`}>
      <h2>
        <span className="settings-card-icon">{icon}</span>
        {title}
      </h2>
      <div className="settings-card-body">{children}</div>
    </section>
  );
}

function PathSettingRow({
  disabled = false,
  id,
  label,
  onBrowse,
  onChange,
  readonlyText = "只读",
  value,
}: {
  disabled?: boolean;
  id: string;
  label: string;
  value: string;
  onBrowse?: () => void;
  onChange?: (value: string) => void;
  readonlyText?: string;
}) {
  return (
    <div className={`path-setting-row ${disabled ? "disabled" : ""}`}>
      <label htmlFor={id}>{label}</label>
      <div className="path-setting-control">
        <input
          id={id}
          value={value}
          title={value}
          disabled={disabled}
          readOnly={disabled}
          onChange={(event) => onChange?.(event.target.value)}
        />
        {onBrowse ? (
          <button className="settings-button browse" disabled={disabled} onClick={onBrowse}>
            浏览
          </button>
        ) : (
          <span className="settings-button browse readonly-placeholder" aria-hidden="true">
            {readonlyText}
          </span>
        )}
      </div>
    </div>
  );
}

function ToggleSetting({
  checked,
  description,
  id,
  label,
  onChange,
}: {
  checked: boolean;
  description?: string;
  id: string;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="toggle-setting">
      <div>
        <label id={`${id}-label`}>{label}</label>
        {description && <p>{description}</p>}
      </div>
      <button
        aria-checked={checked}
        aria-labelledby={`${id}-label`}
        className={`settings-toggle ${checked ? "on" : "off"}`}
        id={id}
        role="switch"
        onClick={() => onChange(!checked)}
      >
        <span aria-hidden="true" />
        <em>{checked ? "开" : "关"}</em>
      </button>
    </div>
  );
}

function HelperText({ children }: { children: ReactNode }) {
  return <p className="settings-helper">{children}</p>;
}

function LanguageChip({ value }: { value: SubtitleLanguage }) {
  return <span className={`settings-chip language ${chipClass(value)}`}>{value}</span>;
}

function StatusChip({ enabled }: { enabled: boolean }) {
  return <span className={`settings-chip status ${enabled ? "enabled" : "disabled"}`}>{enabled ? "已开启" : "已关闭"}</span>;
}

function SummaryItem({ label, tone, value }: { label: string; value: string; tone?: "success" | "muted" }) {
  return (
    <div className="summary-item">
      <span>{label}</span>
      <strong className={tone ?? ""} title={value}>
        {value}
      </strong>
    </div>
  );
}

function languageLabel(value: SubtitleLanguage) {
  return languageOptions.find((option) => option.value === value)?.summary ?? value;
}
