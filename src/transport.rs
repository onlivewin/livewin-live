use crate::packet::Packet;
use crate::{AppName, Event, StreamKey};
use tokio::sync::{broadcast, mpsc, oneshot};

pub type Responder<P> = oneshot::Sender<P>;
pub enum ChannelMessage {
    Create((AppName, StreamKey, Responder<Handle>)),
    Release(AppName),
    Join((AppName, Responder<(Handle, Watcher)>)),
    RegisterTrigger(Event, Trigger),
}

pub type ManagerHandle = mpsc::UnboundedSender<ChannelMessage>;
pub(super) type ChannelReceiver = mpsc::UnboundedReceiver<ChannelMessage>;

pub type Trigger = mpsc::UnboundedSender<(String, Watcher)>;
pub(super) type TriggerHandle = mpsc::UnboundedReceiver<(String, Watcher)>;

pub fn trigger_channel() -> (Trigger, TriggerHandle) {
    mpsc::unbounded_channel()
}

pub enum Message {
    Packet(Packet),
    InitData(
        Responder<(
            Option<Packet>,
            Option<Packet>,
            Option<Packet>,
            Option<Vec<Packet>>,
        )>,
    ),
    Disconnect,
}

pub type Handle = mpsc::UnboundedSender<Message>;
pub(super) type IncomingBroadcast = mpsc::UnboundedReceiver<Message>;
pub(super) type OutgoingBroadcast = broadcast::Sender<Packet>;
pub type Watcher = broadcast::Receiver<Packet>;

pub enum TsMessageQueue {
    Ts(AppName, i64, u8),
}

pub type TsMessageQueueHandle = mpsc::UnboundedSender<TsMessageQueue>;
pub type TsMessageReceiver = mpsc::UnboundedReceiver<TsMessageQueue>;
