use crate::error::{LinkError, Result};
use crate::protocol::Message;
use crate::serialization::{BinarySerializer, BinaryFormat};
use bytes::Bytes;

#[cfg(feature = "async")]
use async_trait::async_trait;

pub trait Transport {
    fn send(&mut self, message: &Message) -> Result<()>;
    fn receive(&mut self) -> Result<Option<Message>>;
    fn close(&mut self) -> Result<()>;
    fn is_connected(&self) -> bool;
}

#[cfg(feature = "async")]
#[async_trait]
pub trait AsyncTransport: Send + Sync {
    async fn send(&mut self, message: &Message) -> Result<()>;
    async fn receive(&mut self) -> Result<Option<Message>>;
    async fn close(&mut self) -> Result<()>;
    fn is_connected(&self) -> bool;
}

pub struct MemoryTransport {
    serializer: BinarySerializer,
    send_buffer: Vec<Bytes>,
    receive_buffer: Vec<Bytes>,
    connected: bool,
}

impl MemoryTransport {
    pub fn new(format: BinaryFormat) -> Self {
        Self {
            serializer: BinarySerializer::new(format),
            send_buffer: Vec::new(),
            receive_buffer: Vec::new(),
            connected: true,
        }
    }

    pub fn create_pair(format: BinaryFormat) -> (Self, Self) {
        let t1 = Self::new(format);
        let t2 = Self::new(format);
        (t1, t2)
    }

    pub fn connect_to(&mut self, other: &mut Self) {
        std::mem::swap(&mut self.send_buffer, &mut other.receive_buffer);
        std::mem::swap(&mut self.receive_buffer, &mut other.send_buffer);
    }

    pub fn get_send_buffer(&self) -> &[Bytes] {
        &self.send_buffer
    }

    pub fn get_receive_buffer(&self) -> &[Bytes] {
        &self.receive_buffer
    }
}

impl Transport for MemoryTransport {
    fn send(&mut self, message: &Message) -> Result<()> {
        if !self.connected {
            return Err(LinkError::ConnectionClosed);
        }

        let data = self.serializer.serialize_message(message)?;
        self.send_buffer.push(data);
        Ok(())
    }

    fn receive(&mut self) -> Result<Option<Message>> {
        if !self.connected {
            return Err(LinkError::ConnectionClosed);
        }

        if self.receive_buffer.is_empty() {
            return Ok(None);
        }

        let data = self.receive_buffer.remove(0);
        let message = self.serializer.deserialize_message(&data)?;
        Ok(Some(message))
    }

    fn close(&mut self) -> Result<()> {
        self.connected = false;
        self.send_buffer.clear();
        self.receive_buffer.clear();
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

pub struct StdioTransport {
    serializer: BinarySerializer,
    connected: bool,
}

impl StdioTransport {
    pub fn new(format: BinaryFormat) -> Self {
        Self {
            serializer: BinarySerializer::new(format),
            connected: true,
        }
    }
}

impl Transport for StdioTransport {
    fn send(&mut self, message: &Message) -> Result<()> {
        if !self.connected {
            return Err(LinkError::ConnectionClosed);
        }

        use std::io::Write;

        let data = self.serializer.serialize_message(message)?;
        let len = data.len() as u32;

        let mut stdout = std::io::stdout();
        stdout.write_all(&len.to_le_bytes())?;
        stdout.write_all(&data)?;
        stdout.flush()?;

        Ok(())
    }

    fn receive(&mut self) -> Result<Option<Message>> {
        if !self.connected {
            return Err(LinkError::ConnectionClosed);
        }

        use std::io::Read;

        let mut stdin = std::io::stdin();
        let mut len_bytes = [0u8; 4];

        match stdin.read_exact(&mut len_bytes) {
            Ok(_) => {},
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        }

        let len = u32::from_le_bytes(len_bytes) as usize;
        let mut buffer = vec![0u8; len];

        stdin.read_exact(&mut buffer)?;

        let message = self.serializer.deserialize_message(&buffer)?;
        Ok(Some(message))
    }

    fn close(&mut self) -> Result<()> {
        self.connected = false;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

#[cfg(feature = "websocket")]
pub mod websocket {
    use super::*;
    use tokio_tungstenite::{
        WebSocketStream,
        tungstenite::Message as WsMessage,
    };
    use tokio::net::TcpStream;
    use futures_util::{SinkExt, StreamExt};

    pub struct WebSocketTransport {
        serializer: BinarySerializer,
        stream: Option<WebSocketStream<TcpStream>>,
    }

    impl WebSocketTransport {
        pub fn new(format: BinaryFormat, stream: WebSocketStream<TcpStream>) -> Self {
            Self {
                serializer: BinarySerializer::new(format),
                stream: Some(stream),
            }
        }
    }

    #[async_trait]
    impl AsyncTransport for WebSocketTransport {
        async fn send(&mut self, message: &Message) -> Result<()> {
            let stream = self.stream.as_mut()
                .ok_or(LinkError::ConnectionClosed)?;

            let data = self.serializer.serialize_message(message)?;
            stream.send(WsMessage::Binary(data.to_vec())).await
                .map_err(|e| LinkError::Transport(e.to_string()))?;

            Ok(())
        }

        async fn receive(&mut self) -> Result<Option<Message>> {
            let stream = self.stream.as_mut()
                .ok_or(LinkError::ConnectionClosed)?;

            match stream.next().await {
                Some(Ok(WsMessage::Binary(data))) => {
                    let message = self.serializer.deserialize_message(&data)?;
                    Ok(Some(message))
                }
                Some(Ok(WsMessage::Close(_))) => {
                    self.stream = None;
                    Err(LinkError::ConnectionClosed)
                }
                Some(Ok(_)) => Ok(None),
                Some(Err(e)) => Err(LinkError::Transport(e.to_string())),
                None => {
                    self.stream = None;
                    Err(LinkError::ConnectionClosed)
                }
            }
        }

        async fn close(&mut self) -> Result<()> {
            if let Some(mut stream) = self.stream.take() {
                stream.close(None).await
                    .map_err(|e| LinkError::Transport(e.to_string()))?;
            }
            Ok(())
        }

        fn is_connected(&self) -> bool {
            self.stream.is_some()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    NotConnected,
    SendFailed,
    ReceiveFailed,
    CloseFailed,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::NotConnected => write!(f, "Not connected"),
            TransportError::SendFailed => write!(f, "Send failed"),
            TransportError::ReceiveFailed => write!(f, "Receive failed"),
            TransportError::CloseFailed => write!(f, "Close failed"),
        }
    }
}

impl std::error::Error for TransportError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::MessageType;

    #[test]
    fn test_memory_transport() {
        let mut transport1 = MemoryTransport::new(BinaryFormat::MessagePack);
        let mut transport2 = MemoryTransport::new(BinaryFormat::MessagePack);

        let message = Message::ping(1);
        transport1.send(&message).unwrap();

        transport1.connect_to(&mut transport2);

        let received = transport2.receive().unwrap().unwrap();
        assert_eq!(message.header.msg_type, received.header.msg_type);
    }

    #[test]
    fn test_transport_close() {
        let mut transport = MemoryTransport::new(BinaryFormat::Json);

        assert!(transport.is_connected());

        transport.close().unwrap();

        assert!(!transport.is_connected());

        let message = Message::ping(1);
        assert!(transport.send(&message).is_err());
    }
}
