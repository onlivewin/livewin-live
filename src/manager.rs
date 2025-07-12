use crate::channel::Channel;
use crate::transport::{
    ChannelMessage, ChannelReceiver, Handle, ManagerHandle, OutgoingBroadcast, Trigger,
};
use crate::user::UserCheck;
use crate::{AppName, Event};
use anyhow::{bail, Result};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, mpsc, RwLock};

pub struct Manager<D>
where
    D: UserCheck + 'static + Send + Sync,
{
    handle: ManagerHandle,
    user_checker: Option<D>,
    incoming: ChannelReceiver,
    channels: Arc<RwLock<HashMap<AppName, (Handle, OutgoingBroadcast)>>>,
    triggers: Arc<RwLock<HashMap<Event, Vec<Trigger>>>>,
    full_gop: bool,
    auth_enable: bool,
}

impl<D> Manager<D>
where
    D: UserCheck + 'static + Send + Sync,
{
    pub fn new(user_checker: Option<D>, full_gop: bool, auth_enable: bool) -> Self {
        let (handle, incoming) = mpsc::unbounded_channel();
        let channels = Arc::new(RwLock::new(HashMap::new()));
        let triggers = Arc::new(RwLock::new(HashMap::new()));

        Self {
            handle,
            user_checker,
            incoming,
            channels,
            triggers,
            full_gop,
            auth_enable,
        }
    }

    pub fn handle(&self) -> ManagerHandle {
        self.handle.clone()
    }

    async fn process_message(&mut self, message: ChannelMessage) -> Result<()> {
        match message {
            ChannelMessage::Create((name, key, responder)) => {
                //验证用户
                if self.auth_enable {
                    self.auth(&name, &key).await?;
                }

                let (handle, incoming) = mpsc::unbounded_channel();
                let (outgoing, _watcher) = broadcast::channel(64);
                let mut sessions = self.channels.write().await;
                sessions.insert(name.clone(), (handle.clone(), outgoing.clone()));

                let triggers = self.triggers.read().await;
                if let Some(event_triggers) = triggers.get("create_session") {
                    for trigger in event_triggers {
                        trigger.send((name.clone(), outgoing.subscribe()))?;
                    }
                }

                let full_gop = self.full_gop;
                let name_copy = name.clone();
                tokio::spawn(async move {
                    Channel::new(name_copy, incoming, outgoing, full_gop)
                        .run()
                        .await;
                });

                if let Err(_) = responder.send(handle) {
                    bail!("Failed to send response");
                }
            }
            ChannelMessage::Join((name, responder)) => {
                let sessions = self.channels.read().await;
                if let Some((handle, watcher)) = sessions.get(&name) {
                    if let Err(_) = responder.send((handle.clone(), watcher.subscribe())) {
                        bail!("Failed to send response");
                    }
                } else {
                    log::warn!("Channel not found: {}", name);
                    // Send an error response instead of ignoring
                    drop(responder);
                }
            }
            ChannelMessage::Release(name) => {
                let mut sessions = self.channels.write().await;
                sessions.remove(&name);
            }
            ChannelMessage::RegisterTrigger(event, trigger) => {
                log::debug!("Registering trigger for {}", event);
                let mut triggers = self.triggers.write().await;
                triggers.entry(event).or_insert_with(Vec::new).push(trigger);
            }
        }

        Ok(())
    }

    pub async fn run(mut self) {
        while let Some(message) = self.incoming.recv().await {
            if let Err(err) = self.process_message(message).await {
                log::error!("{}", err);
            };
        }
    }

    async fn auth(&self, name: &str, key: &str) -> Result<()> {
        if let Some(checker) = &self.user_checker {
            if key.is_empty() {
                bail!("Stream key can not be empty");
            }
            if let Ok(Some(k)) = checker.get_key(name).await {
                if k == key {
                    return Ok(());
                }
            }
            bail!("Stream key {} not permitted for {}", key, name);
        }
        Ok(())
    }
}
