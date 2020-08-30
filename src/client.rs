use crate::protocol::{Connection, Message};
use std::net::SocketAddrV4;
use tokio::net::TcpStream;

pub struct Client {
    pub connection: Option<Connection>,
}

impl Client {
    pub async fn connect(target: SocketAddrV4) -> Self {
        println!("Client connecting to {}", target);

        let stream = TcpStream::connect(target).await.unwrap();

        println!("Client connected.");

        let connection = Connection::new_encrypted(stream).await;

        Client {
            connection: Some(connection),
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        match self.connection.take() {
            None => {}
            Some(connection) => {
                tokio::spawn(async move {
                    let mut connection = connection;
                    connection.send_message(Message::Disconnect).await;
                });
            }
        }
    }
}
