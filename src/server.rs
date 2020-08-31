use crate::{
    networking::Connection,
    protocol::{ClientMessage, GreetingResponse, ReqRes},
};
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

struct ServerConnection(Connection);

impl ServerConnection {
    pub fn new(connetion: Connection) -> Self {
        ServerConnection(connetion)
    }

    pub async fn receive(&mut self, buffer: &mut Vec<u8>) -> ClientMessage {
        self.0.stream.receive_bincode(buffer).await
    }

    pub async fn respond<S: ReqRes>(&mut self, _req: S, res: S::Response) {
        self.0.stream.send_bincode(&res).await;
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
        mut connection: ServerConnection,
        session: SharedSession,
    ) {
        let session_reader = session.read().await;
        let address = session_reader.address.clone();
        drop(session_reader);

        let mut message_buffer = Vec::new();

        loop {
            let message = connection.receive(&mut message_buffer).await;

            match message {
                ClientMessage::Greeting(greeting) => {
                    connection
                        .respond(greeting, GreetingResponse::ProtocolOk)
                        .await;
                }
                ClientMessage::Disconnect => {
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
                        let connection = ServerConnection::new(connection);

                        let session = Arc::new(RwLock::new(Session { address }));

                        let mut server_writer = closure_server.write().await;
                        server_writer.sessions.insert(address, session.clone());
                        drop(server_writer);

                        task::spawn(async move {
                            Self::handle_client(sender, connection, session).await;
                        });
                    },
                    Some(control_message) = receiver.recv() => {
                        match control_message {
                            ControlMessage::Shutdown => {
                                todo!()
                            },
                            ControlMessage::Disconnect(address) => { }
                        }
                    }
                }
            }
        });

        server
    }
}
