use crate::protocol::{Connection, Message};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::{select, task};

pub trait FileSystem: Send + Sync {
    fn list_files(&self, path: &std::path::Path, output: &mut Vec<std::path::PathBuf>);
}

pub struct MockFileSystem {}

impl MockFileSystem {
    pub fn new() -> Self {
        MockFileSystem {}
    }
}

impl FileSystem for MockFileSystem {
    fn list_files(&self, path: &std::path::Path, output: &mut Vec<std::path::PathBuf>) {
        todo!()
    }
}

pub struct Session {
    address: SocketAddr,
}

type SharedSession = Arc<RwLock<Session>>;

pub struct Server {
    fs: Box<dyn FileSystem>,
    pub sessions: HashMap<SocketAddr, SharedSession>,
}

#[derive(Debug)]
enum ControlMessage {
    Shutdown,
    Disconnect(SocketAddr),
}

impl Server {
    async fn handle_client(
        mut server_channel: tokio::sync::mpsc::Sender<ControlMessage>,
        mut connection: Connection,
        session: SharedSession,
    ) {
        let session_reader = session.read().await;
        let address = session_reader.address.clone();
        drop(session_reader);

        loop {
            let message = connection.read_message().await;

            match message {
                Message::Greeting { .. } => {
                    connection
                        .send_message(Message::Greeting {
                            protocol_version: 1,
                        })
                        .await;
                }
                Message::Disconnect => {
                    println!("Client {} disconnecting.", address);

                    server_channel
                        .send(ControlMessage::Disconnect(address))
                        .await
                        .unwrap();
                    break;
                }
            }
        }
    }

    pub fn start_new(fs: Box<dyn FileSystem>, mut socket: TcpListener) -> Arc<RwLock<Server>> {
        let server = Server {
            fs,
            sessions: HashMap::new(),
        };

        let (sender, mut receiver) = tokio::sync::mpsc::channel(4);

        let server = Arc::new(RwLock::new(server));
        let closure_server = server.clone();

        task::spawn(async move {
            loop {
                let sender = sender.clone();

                select! {
                    Ok((stream, address)) = socket.accept() => {
                        println!("Connection received from {}", address);
                        let connection = Connection::new_encrypted(stream).await;

                        let session = Arc::new(RwLock::new(Session { address }));

                        let mut server_writer = closure_server.write().await;
                        server_writer.sessions.insert(address, session.clone());
                        drop(server_writer);

                        println!("Client added :D {:?}", address);

                        task::spawn(async move {
                            Self::handle_client(sender, connection, session).await;
                        });
                    },
                    control_message = receiver.recv() => {
                        println!(":DDD")
                    }
                }
            }
        });

        server
    }
}
