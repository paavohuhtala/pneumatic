use crate::crypto::EncryptedStream;
use tokio::net::TcpStream;

// TODO: Is this wrapper necessary?
pub struct Connection {
    pub stream: EncryptedStream,
}

impl Connection {
    pub async fn new_encrypted(stream: TcpStream) -> Self {
        let stream = EncryptedStream::new(stream).await;
        Connection { stream }
    }
}
