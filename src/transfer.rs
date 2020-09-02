use crate::config::ServerConfig;
use async_trait::async_trait;
use futures::future;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};
use tokio::fs::read_dir;

#[derive(Debug)]
pub enum DiscoveryMessage {
    Files(Vec<FileMetadata>),
}

#[async_trait]
pub trait FileSystem {
    type Metadata;

    fn root(&self) -> &std::path::Path;
    fn convert_metadata(&self, path: &std::path::Path, metadata: Self::Metadata) -> FileMetadata;

    async fn discover_files_recursively(
        self: Arc<Self>,
        path: PathBuf,
        output: tokio::sync::mpsc::Sender<DiscoveryMessage>,
    ) -> Result<(), anyhow::Error>;
}

pub struct StdFilesystem {
    root: std::path::PathBuf,
}

impl StdFilesystem {
    pub fn new(root: impl AsRef<std::path::Path>) -> Self {
        let root = root.as_ref().to_owned();
        StdFilesystem { root }
    }
}

#[async_trait]
impl FileSystem for StdFilesystem {
    type Metadata = std::fs::Metadata;

    fn root(&self) -> &Path {
        &self.root
    }

    fn convert_metadata(&self, path: &std::path::Path, metadata: Self::Metadata) -> FileMetadata {
        FileMetadata {
            relative_path: path.strip_prefix(&self.root).unwrap().to_owned(),
            created_at: metadata.created().ok(),
            modified_at: metadata.modified().ok(),
            uncompressed_size: metadata.len(),
        }
    }

    async fn discover_files_recursively(
        self: Arc<Self>,
        path: PathBuf,
        mut output: tokio::sync::mpsc::Sender<DiscoveryMessage>,
    ) -> Result<(), anyhow::Error> {
        let mut file_stream = read_dir(path).await?;
        let mut subtasks = Vec::new();
        let mut files = Vec::new();

        while let Some(entry) = file_stream.next_entry().await? {
            let file_type = entry.file_type().await?;
            let path = entry.path();

            if file_type.is_dir() {
                let output = output.clone();
                let fs = self.clone();

                let handle = tokio::spawn(async move {
                    fs.discover_files_recursively(path, output).await.unwrap()
                });

                subtasks.push(handle);
            } else {
                let metadata = entry.metadata().await?;
                let metadata = self.convert_metadata(&path, metadata);
                files.push(metadata);
            }
        }

        output.send(DiscoveryMessage::Files(files)).await?;

        future::join_all(subtasks).await;

        Ok(())
    }
}

struct Batch {}

pub struct TransferPlan {}

impl TransferPlan {
    pub fn create(mut files: Vec<FileMetadata>, config: &ServerConfig) -> Self {
        files.sort_unstable_by_key(|k| k.uncompressed_size);

        let small_file_threshold = config.get_small_file_threshold();
        let first_non_small_file_index = files
            .iter()
            .enumerate()
            .skip_while(|(_, file)| file.uncompressed_size < small_file_threshold)
            .next()
            .map(|(i, _)| i)
            .unwrap();

        let small_files = &files[0..first_non_small_file_index];

        let large_file_threshold = config.get_large_file_threshold();
        let first_large_file_index = files
            .iter()
            .enumerate()
            .skip(first_non_small_file_index)
            .skip_while(|(_, file)| file.uncompressed_size < large_file_threshold)
            .next()
            .map(|(i, _)| i)
            .unwrap();

        let single_chunk_files = &files[first_non_small_file_index..first_large_file_index];

        let large_files = &files[first_large_file_index..];

        println!(
            "Small files: {}\nSingle chunk files: {}\nLarge files: {}",
            small_files.len(),
            single_chunk_files.len(),
            large_files.len()
        );

        todo!();
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileMetadata {
    relative_path: PathBuf,
    created_at: Option<SystemTime>,
    modified_at: Option<SystemTime>,
    uncompressed_size: u64,
}
