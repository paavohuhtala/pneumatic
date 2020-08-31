use crate::{networking::Connection, protocol::ClientMessage};
use std::net::SocketAddrV4;
use tokio::net::TcpStream;

pub struct Client {
    connection: Option<Connection>,
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

    async fn send_message_stream(connection: &mut Connection, message: ClientMessage) {
        connection.stream.send_bincode(&message).await;
    }

    pub async fn send_message(&mut self, message: ClientMessage) {
        match self.connection.as_mut() {
            None => {}
            Some(connection) => Self::send_message_stream(connection, message).await,
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
                    Self::send_message_stream(&mut connection, ClientMessage::Disconnect).await
                });
            }
        }
    }
}
