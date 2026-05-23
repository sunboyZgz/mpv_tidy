use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("目录不存在：{0}")]
    MissingDirectory(PathBuf),
    #[error("文件不存在：{0}")]
    MissingFile(PathBuf),
    #[error("文件系统操作失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("目录扫描失败：{0}")]
    WalkDir(#[from] walkdir::Error),
    #[error("序列化失败：{0}")]
    Serde(#[from] serde_json::Error),
    #[error("没有可整理的条目")]
    EmptyOrganizePlan,
    #[error("MPV 启动失败：{0}")]
    MpvLaunch(String),
    #[error("本地动漫库保存失败：{0}")]
    LibrarySave(String),
}

pub type AppResult<T> = Result<T, AppError>;

pub fn to_user_error(error: AppError) -> String {
    error.to_string()
}
