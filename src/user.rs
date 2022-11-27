use anyhow::{bail, Result};
use async_trait::async_trait;
use redis::Commands;

#[async_trait]
pub trait UserCheck {
    async fn get_key(&self, name: &str) -> Result<Option<String>>;
    async fn delete_key(&self, key: &str) -> Result<()>;
}

#[derive(Clone)]
pub struct Redis {
    pub client: redis::Client,
}

impl Redis {
    pub fn new(url: &str) -> redis::RedisResult<Self> {
        let client = redis::Client::open(url)?;
        Ok(Self { client })
    }
}

#[async_trait]
impl UserCheck for Redis {
    async fn get_key(&self, name: &str) -> Result<Option<String>> {
        if let Ok(mut conn) = self.client.get_connection() {
            if let Ok(ret) = conn.get(name) {
                return Ok(Some(ret));
            }
        }
        bail!("redis connect err")
    }

    async fn delete_key(&self, key: &str) -> Result<()> {
        if let Ok(mut conn) = self.client.get_connection() {
            conn.del(key)?;
        }
        Ok(())
    }
}
