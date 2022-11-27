use crate::user::Redis;
use anyhow::{bail, Result};
use async_trait::async_trait;

#[async_trait]
pub trait Sender {
    async fn send(&self, key: &str, data: &str) -> Result<()>;
}

#[async_trait]
impl Sender for Redis {
    async fn send(&self, key: &str, data: &str) -> Result<()> {
        if let Ok(mut conn) = self.client.get_connection() {
            redis::Cmd::new()
                .arg("lpush")
                .arg(key)
                .arg(data)
                .query(&mut conn)?;
            return Ok(());
        }
        bail!("redis connect err")
    }
}
