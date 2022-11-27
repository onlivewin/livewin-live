use crate::connection::Connection;
use crate::ManagerHandle;
use anyhow::Result;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;

pub struct Service {
    manager_handle: ManagerHandle,
    client_id: u64,
}

impl Service {
    pub fn new(manager_handle: ManagerHandle) -> Self {
        Self {
            manager_handle,
            client_id: 0,
        }
    }
    pub async fn run(mut self, port: i32) {
        if let Err(err) = self.handle_rtmp(port).await {
            log::error!("{}", err);
        }
    }

    async fn handle_rtmp(&mut self, port: i32) -> Result<()> {
        let addr = format!("[::]:{}", port);
        let listener = TcpListener::bind(&addr).await?;
        log::info!("Listening for RTMP connections on {}", addr);
        loop {
            let (tcp_stream, _addr) = listener.accept().await?;
            self.process(tcp_stream);
            self.client_id += 1;
        }
    }

    fn process<S>(&self, stream: S)
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
    {
        log::info!("New client connection: {}", &self.client_id);
        let id = self.client_id;
        let conn = Connection::new(id, stream, self.manager_handle.clone());

        tokio::spawn(async move {
            if let Err(err) = conn.run().await {
                log::error!("{}", err);
            }
        });
    }
}
