use pneumatic::server::MockFileSystem;
use pneumatic::{client::Client, protocol::Message, server::Server};
use std::{
    error::Error,
    net::{Ipv4Addr, SocketAddrV4},
};
use tokio::net::TcpListener;

// TODO: This test is unreliable and prone to race conditions
#[tokio::test(threaded_scheduler)]
async fn connect_then_dc() -> Result<(), Box<dyn Error>> {
    let fs = MockFileSystem::new();
    let address = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 2020);
    let tcp = TcpListener::bind(address).await?;

    let server = Server::start_new(Box::new(fs), tcp);
    let mut client = Client::connect(address).await;

    client
        .connection
        .as_mut()
        .unwrap()
        .send_message(Message::Greeting {
            protocol_version: 1,
        })
        .await;

    let server_reader = server.read().await;
    assert_eq!(server_reader.sessions.len(), 1);

    drop(client);

    Ok(())
}
