use std::path::PathBuf;

use crate::codec::flv::writer::Writer;
use crate::transport::{trigger_channel, ChannelMessage, ManagerHandle, Watcher};
use chrono::prelude::*;
use anyhow::Result;

struct FlvWriter {
    writer: Writer,
    watcher: Watcher,
}

impl FlvWriter {
    fn new(writer: Writer, watcher: Watcher) -> Self {
        Self { writer, watcher }
    }
    async fn run(&mut self) -> std::io::Result<()> {
        while let Ok(packet) = self.watcher.recv().await {
            self.writer.write(&packet).await?
        }
        Ok(())
    }
}

pub struct Service {
    manager_handle: ManagerHandle,
    flv_data_path: String,
}

impl Service {
    pub fn new(manager_handle: ManagerHandle, flv_data_path: String) -> Self {
        Self {
            manager_handle,
            flv_data_path,
        }
    }

    pub async fn run(self)->Result<()> {

        let stream_path = PathBuf::from(self.flv_data_path.clone());
        super::prepare_stream_directory(&stream_path)?;

        let (trigger, mut trigger_handle) = trigger_channel();
        if let Err(_) = self
            .manager_handle
            .send(ChannelMessage::RegisterTrigger("create_session", trigger))
        {
            log::error!("Failed to register session trigger");
            return Ok(());
        }

        while let Some((app_name, watcher)) = trigger_handle.recv().await {
            let local: DateTime<Local> = Local::now();
           
            let stream_path = PathBuf::from(self.flv_data_path.clone());
            let stream_path = stream_path.join(app_name.clone());
            super::prepare_stream_directory(&stream_path)?;
            let flv_path = format!(
                "{}/{}/{}.flv",
                self.flv_data_path,
                app_name,
                local.timestamp()
            );
            match Writer::new(flv_path).await {
                Ok(writer) => {
                    let mut flv_writer = FlvWriter::new(writer, watcher);
                    tokio::spawn(async move { flv_writer.run().await.unwrap() });
                }
                Err(why) => log::error!("Failed to create writer: {:?}", why),
            }
        }
        return Ok(())
    }
}
