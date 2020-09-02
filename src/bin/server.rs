use pneumatic;
use pneumatic::{
    config::ServerConfig,
    transfer::{DiscoveryMessage, FileSystem, TransferPlan},
};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{self, Duration},
};
use time::Instant;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    let root_path = args.get(1).expect("Expected path as the first argument");
    let root_path = PathBuf::from(root_path);

    let begin = time::Instant::now();

    let fs = pneumatic::transfer::StdFilesystem::new(&root_path);
    let fs_arc = Arc::new(fs);

    let (sender, mut receiver) = tokio::sync::mpsc::channel(16);

    let discover = tokio::spawn(async move {
        fs_arc
            .discover_files_recursively(root_path, sender)
            .await
            .unwrap();
    });

    let reporter = tokio::spawn(async move {
        let mut all_files = Vec::new();

        loop {
            match receiver.recv().await {
                None => break,
                Some(message) => match message {
                    DiscoveryMessage::Files(mut files) => {
                        all_files.append(&mut files);
                    }
                },
            }
        }

        all_files
    });

    let (all_files, _) = futures::join!(reporter, discover);

    let end = Instant::now();

    let took: Duration = end - begin;

    println!(
        "Discovered {} files in {}ms",
        all_files.as_ref().unwrap().len(),
        took.as_millis()
    );

    TransferPlan::create(all_files.unwrap(), &ServerConfig::default());
}
