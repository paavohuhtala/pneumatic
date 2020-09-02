use crate::config::ServerConfig;
use async_trait::async_trait;
use crossbeam::queue::SegQueue;
use futures::future;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
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
        output: tokio::sync::mpsc::Sender<DiscoveryMessage>,
    ) -> Result<(), anyhow::Error> {
        let processing_queue = Arc::new(SegQueue::new());
        let folders_to_process = Arc::new(AtomicU64::new(1));

        processing_queue.push(path);

        let mut tasks = Vec::new();

        const CONCURRENCY_LIMIT: u32 = 16;

        for _ in 0..CONCURRENCY_LIMIT {
            let fs = self.clone();
            let queue = processing_queue.clone();
            let mut output = output.clone();
            let folders_to_process = folders_to_process.clone();

            let task = tokio::spawn(async move {
                loop {
                    if folders_to_process.load(Ordering::SeqCst) == 0 {
                        break;
                    }

                    let path: PathBuf = match queue.pop() {
                        Ok(path) => path,
                        Err(_) => {
                            tokio::task::yield_now().await;
                            continue;
                        }
                    };

                    let mut file_stream = read_dir(path).await?;
                    let mut files = Vec::new();

                    while let Some(entry) = file_stream.next_entry().await? {
                        let file_type = entry.file_type().await?;
                        let path = entry.path();

                        if file_type.is_dir() {
                            folders_to_process.fetch_add(1, Ordering::SeqCst);
                            queue.push(path);
                        } else {
                            let metadata = entry.metadata().await?;
                            let metadata = fs.convert_metadata(&path, metadata);
                            files.push(metadata);
                        }
                    }

                    output.send(DiscoveryMessage::Files(files)).await?;

                    folders_to_process.fetch_sub(1, Ordering::SeqCst);
                }

                let ret: Result<(), anyhow::Error> = Ok(());
                ret
            });

            tasks.push(task);
        }

        future::join_all(tasks).await;

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
