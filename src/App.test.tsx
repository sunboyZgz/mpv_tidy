import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { LocalAnimePage } from "./features/localAnime/LocalAnimePage";
import { ProjectHomePage } from "./features/projectHome/ProjectHomePage";
import type { EpisodeMatch, LocalAnimeLibraryEntry, ScanAndMatchResult, ScannedVideo } from "./types";

const invokeMock = vi.hoisted(() => vi.fn());
const openMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: openMock,
}));

beforeEach(() => {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    configurable: true,
    value: {},
  });
  invokeMock.mockImplementation((command: string) => {
    if (command === "load_local_library") {
      return Promise.resolve({
        appVersion: "0.1.0",
        entries: [makeLibraryEntry("Jujutsu Kaisen", 4), makeLibraryEntry("Steins;Gate", 3)],
      });
    }
    if (command === "launch_mpv" || command === "reveal_path") {
      return Promise.resolve({ processId: 1, argumentCount: 4 });
    }
    return Promise.resolve(null);
  });
  openMock.mockReset();
});

async function renderLocalAnimePage() {
  const user = userEvent.setup();
  render(<LocalAnimePage showToast={vi.fn()} syncedEntry={null} />);
  await screen.findByRole("heading", { name: "我的动漫库（2）" });
  await waitFor(() => expect(screen.getByTestId("playback-panel")).toHaveClass("open"));
  return user;
}

describe("App shell", () => {
  it("does not render duplicated window controls inside the web UI", () => {
    render(<App />);

    expect(screen.queryByText("□")).not.toBeInTheDocument();
    expect(screen.queryByText("×")).not.toBeInTheDocument();
  });
});

describe("Local Anime page", () => {
  it("filters and clears the local anime list from the search box", async () => {
    const user = await renderLocalAnimePage();

    await user.type(screen.getByPlaceholderText("搜索本地标题 / 别名 / 标签..."), "Steins");

    expect(screen.getByRole("heading", { name: "我的动漫库（1）" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Steins;Gate/ })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "清空搜索" }));

    expect(screen.getByRole("heading", { name: "我的动漫库（2）" })).toBeInTheDocument();
  });

  it("selects an anime from the library and updates the detail area", async () => {
    const user = await renderLocalAnimePage();

    await user.click(screen.getByRole("button", { name: /Steins;Gate/ }));

    expect(screen.getByRole("heading", { name: "Steins;Gate S01" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "剧集列表（3）" })).toBeInTheDocument();
  });

  it("closes the playback drawer and reopens it when an episode is selected", async () => {
    const user = await renderLocalAnimePage();
    const panel = screen.getByTestId("playback-panel");

    expect(panel).toHaveClass("open");

    await user.click(screen.getByRole("button", { name: "收起播放设置" }));
    expect(panel).not.toHaveClass("open");

    await user.click(screen.getByRole("button", { name: "展开播放设置" }));
    expect(panel).toHaveClass("open");

    await user.click(screen.getByRole("button", { name: "收起播放设置" }));
    await user.click(screen.getByRole("row", { name: /S01E04/ }));
    expect(panel).toHaveClass("open");
  });

  it("prevents playback when primary and secondary subtitles point to the same file", async () => {
    const user = await renderLocalAnimePage();
    const primarySubtitle = screen.getByLabelText("主字幕") as HTMLSelectElement;

    await waitFor(() => expect(primarySubtitle.value).not.toBe(""));
    await user.selectOptions(screen.getByLabelText("副字幕"), primarySubtitle.value);

    expect(screen.getByText("主字幕和副字幕不能选择同一个文件。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /用 MPV 播放/ })).toBeDisabled();
  });

  it("adjusts and resets subtitle offsets", async () => {
    const user = await renderLocalAnimePage();
    const primaryOffset = screen.getByLabelText("主字幕偏移");

    expect(primaryOffset).toHaveValue("0.0");

    await user.click(screen.getByRole("button", { name: "主字幕偏移增加" }));
    expect(primaryOffset).toHaveValue("0.1");

    await user.click(screen.getByRole("button", { name: "重置偏移" }));
    expect(primaryOffset).toHaveValue("0.0");
  });
});

describe("Project home parser evidence", () => {
  it("renders parser notes in the detail drawer after scanning", async () => {
    const user = userEvent.setup();
    openMock.mockResolvedValueOnce("D:\\Anime\\videos").mockResolvedValueOnce(["D:\\Anime\\subs"]);
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_and_match") {
        return Promise.resolve(makeParserEvidenceScanResult());
      }
      return Promise.resolve(null);
    });

    render(<ProjectHomePage showToast={vi.fn()} onLibraryEntrySaved={vi.fn()} />);

    await user.click(screen.getAllByRole("button", { name: "更换目录" })[0]);
    await user.click(screen.getByRole("button", { name: "添加字幕目录" }));
    await user.click(screen.getByRole("button", { name: "开始扫描" }));

    expect(await screen.findByText("解析证据 · 低置信")).toBeInTheDocument();
    expect(screen.getByText("存在多个接近的 episode 候选，需要手动确认。")).toBeInTheDocument();
  });
});

function makeLibraryEntry(projectName: string, episodeCount: number): LocalAnimeLibraryEntry {
  return {
    projectName,
    season: "S01",
    outputDir: `D:\\Anime Library\\${projectName} S01`,
    mode: "copy",
    episodeCount,
    organizedAtUnix: 1_777_000_000,
    episodes: Array.from({ length: episodeCount }, (_, index) => {
      const episodeKey = `S01E${String(index + 1).padStart(2, "0")}`;
      return {
        episodeKey,
        videoPath: `D:\\Anime Library\\${projectName} S01\\videos\\${projectName} ${episodeKey}.mkv`,
        primarySubtitlePath: `D:\\Anime Library\\${projectName} S01\\subs\\zh-Hans\\${projectName} ${episodeKey}.zh-Hans.ass`,
        secondarySubtitlePath: `D:\\Anime Library\\${projectName} S01\\subs\\en\\${projectName} ${episodeKey}.en.srt`,
        subtitleCount: 2,
        status: "matched",
      };
    }),
  };
}

function makeParserEvidenceScanResult(): ScanAndMatchResult {
  const video = makeScannedVideoWithParserNotes();
  const match: EpisodeMatch = {
    episode: { season: 1, episode: 1 },
    episodeKey: "S01E01",
    video,
    primarySubtitle: null,
    secondarySubtitle: null,
    candidates: [],
    status: "pendingFix",
    notes: [],
  };

  return {
    scan: { videos: [video], subtitles: [] },
    matches: [match],
    unprocessedVideos: [],
    unprocessedSubtitles: [],
  };
}

function makeScannedVideoWithParserNotes(): ScannedVideo {
  return {
    path: "D:\\Anime\\videos\\A-01-02-03.mkv",
    fileName: "A-01-02-03.mkv",
    extension: "mkv",
    fileSizeBytes: 1024,
    episode: { season: 1, episode: 1 },
    episodeKey: "S01E01",
    confidence: 68,
    parseStatus: "lowConfidence",
    parseNotes: ["存在多个接近的 episode 候选，需要手动确认。"],
    parseCandidates: [
      {
        episode: { season: 1, episode: 1 },
        episodeKey: "S01E01",
        confidence: 68,
        source: "template",
        note: "同组文件模板归纳出变化数字槽。",
      },
    ],
  };
}
