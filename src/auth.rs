use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use async_trait::async_trait;
use crate::errors::{Result, StreamingError};

/// 用户权限
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Permission {
    /// 推流权限
    Publish,
    /// 观看权限
    Subscribe,
    /// 管理权限
    Admin,
    /// 指标查看权限
    ViewMetrics,
    /// 健康检查权限
    ViewHealth,
}

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub permissions: Vec<Permission>,
    pub stream_keys: Vec<String>,
    pub created_at: u64,
    pub last_login: Option<u64>,
    pub active: bool,
}

impl User {
    pub fn new(id: String, username: String) -> Self {
        Self {
            id,
            username,
            permissions: vec![Permission::Subscribe], // 默认只有观看权限
            stream_keys: Vec::new(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            last_login: None,
            active: true,
        }
    }

    pub fn with_permissions(mut self, permissions: Vec<Permission>) -> Self {
        self.permissions = permissions;
        self
    }

    pub fn with_stream_keys(mut self, stream_keys: Vec<String>) -> Self {
        self.stream_keys = stream_keys;
        self
    }

    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.active && (
            self.permissions.contains(permission) ||
            self.permissions.contains(&Permission::Admin)
        )
    }

    pub fn can_publish_to_stream(&self, stream_key: &str) -> bool {
        self.active && 
        self.has_permission(&Permission::Publish) &&
        (self.stream_keys.is_empty() || self.stream_keys.contains(&stream_key.to_string()))
    }

    pub fn update_last_login(&mut self) {
        self.last_login = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
    }
}

/// 认证令牌
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub user_id: String,
    pub issued_at: u64,
    pub expires_at: u64,
    pub permissions: Vec<Permission>,
}

impl AuthToken {
    pub fn new(user_id: String, permissions: Vec<Permission>, ttl: Duration) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            user_id,
            issued_at: now,
            expires_at: now + ttl.as_secs(),
            permissions,
        }
    }

    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now >= self.expires_at
    }

    pub fn has_permission(&self, permission: &Permission) -> bool {
        !self.is_expired() && (
            self.permissions.contains(permission) ||
            self.permissions.contains(&Permission::Admin)
        )
    }
}

/// 认证提供者接口
#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn authenticate(&self, username: &str, password: &str) -> Result<User>;
    async fn validate_token(&self, token: &str) -> Result<AuthToken>;
    async fn create_token(&self, user: &User, ttl: Duration) -> Result<String>;
    async fn revoke_token(&self, token: &str) -> Result<()>;
    async fn get_user(&self, user_id: &str) -> Result<Option<User>>;
    async fn update_user(&self, user: &User) -> Result<()>;
}

/// 内存认证提供者（用于演示和测试）
pub struct MemoryAuthProvider {
    users: Arc<RwLock<HashMap<String, User>>>,
    tokens: Arc<RwLock<HashMap<String, AuthToken>>>,
    credentials: Arc<RwLock<HashMap<String, String>>>, // username -> password
}

impl MemoryAuthProvider {
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            tokens: Arc::new(RwLock::new(HashMap::new())),
            credentials: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_user(&self, username: String, password: String, user: User) -> Result<()> {
        let mut users = self.users.write().await;
        let mut credentials = self.credentials.write().await;
        
        if users.contains_key(&user.id) {
            return Err(StreamingError::InvalidRequest {
                message: format!("User with ID {} already exists", user.id),
            });
        }

        if credentials.contains_key(&username) {
            return Err(StreamingError::InvalidRequest {
                message: format!("Username {} already exists", username),
            });
        }

        users.insert(user.id.clone(), user);
        credentials.insert(username, password);
        Ok(())
    }

    fn generate_token(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        SystemTime::now().hash(&mut hasher);
        format!("token_{:x}", hasher.finish())
    }
}

impl Default for MemoryAuthProvider {
    fn default() -> Self {
        let provider = Self::new();

        // 添加默认管理员用户
        let admin_user = User::new("admin".to_string(), "admin".to_string())
            .with_permissions(vec![Permission::Admin])
            .with_stream_keys(vec!["admin_stream".to_string()]);

        // 添加默认发布者用户
        let publisher_user = User::new("publisher".to_string(), "publisher".to_string())
            .with_permissions(vec![Permission::Publish, Permission::Subscribe])
            .with_stream_keys(vec!["test_stream".to_string(), "demo_stream".to_string()]);

        // 添加默认观看者用户
        let viewer_user = User::new("viewer".to_string(), "viewer".to_string())
            .with_permissions(vec![Permission::Subscribe]);

        // 同步添加用户（简化版本）
        let users = provider.users.clone();
        let credentials = provider.credentials.clone();

        tokio::spawn(async move {
            {
                let mut users_guard = users.write().await;
                let mut creds_guard = credentials.write().await;

                users_guard.insert("admin".to_string(), admin_user);
                users_guard.insert("publisher".to_string(), publisher_user);
                users_guard.insert("viewer".to_string(), viewer_user);

                creds_guard.insert("admin".to_string(), "admin123".to_string());
                creds_guard.insert("publisher".to_string(), "pub123".to_string());
                creds_guard.insert("viewer".to_string(), "view123".to_string());
            }
        });

        provider
    }
}

#[async_trait]
impl AuthProvider for MemoryAuthProvider {
    async fn authenticate(&self, username: &str, password: &str) -> Result<User> {
        let credentials = self.credentials.read().await;
        let users = self.users.read().await;

        if let Some(stored_password) = credentials.get(username) {
            if stored_password == password {
                // 找到对应的用户
                for user in users.values() {
                    if user.username == username && user.active {
                        let mut user = user.clone();
                        user.update_last_login();
                        return Ok(user);
                    }
                }
            }
        }

        Err(StreamingError::AuthenticationFailed {
            stream_name: username.to_string(),
        })
    }

    async fn validate_token(&self, token: &str) -> Result<AuthToken> {
        let tokens = self.tokens.read().await;
        
        if let Some(auth_token) = tokens.get(token) {
            if !auth_token.is_expired() {
                return Ok(auth_token.clone());
            }
        }

        Err(StreamingError::AuthenticationFailed {
            stream_name: "invalid_token".to_string(),
        })
    }

    async fn create_token(&self, user: &User, ttl: Duration) -> Result<String> {
        let token_str = self.generate_token();
        let auth_token = AuthToken::new(user.id.clone(), user.permissions.clone(), ttl);
        
        let mut tokens = self.tokens.write().await;
        tokens.insert(token_str.clone(), auth_token);
        
        Ok(token_str)
    }

    async fn revoke_token(&self, token: &str) -> Result<()> {
        let mut tokens = self.tokens.write().await;
        tokens.remove(token);
        Ok(())
    }

    async fn get_user(&self, user_id: &str) -> Result<Option<User>> {
        let users = self.users.read().await;
        Ok(users.get(user_id).cloned())
    }

    async fn update_user(&self, user: &User) -> Result<()> {
        let mut users = self.users.write().await;
        users.insert(user.id.clone(), user.clone());
        Ok(())
    }
}

/// 认证中间件
pub struct AuthMiddleware {
    provider: Arc<dyn AuthProvider>,
}

impl AuthMiddleware {
    pub fn new(provider: Arc<dyn AuthProvider>) -> Self {
        Self { provider }
    }

    /// 从请求头中提取令牌
    pub fn extract_token_from_header<'a>(&self, auth_header: &'a str) -> Option<&'a str> {
        if auth_header.starts_with("Bearer ") {
            Some(&auth_header[7..])
        } else {
            None
        }
    }

    /// 验证令牌并检查权限
    pub async fn verify_permission(&self, token: &str, required_permission: &Permission) -> Result<AuthToken> {
        let auth_token = self.provider.validate_token(token).await?;
        
        if !auth_token.has_permission(required_permission) {
            return Err(StreamingError::AuthorizationFailed {
                user: auth_token.user_id,
                stream_name: format!("{:?}", required_permission),
            });
        }

        Ok(auth_token)
    }

    /// 验证流推送权限
    pub async fn verify_stream_publish(&self, token: &str, stream_key: &str) -> Result<User> {
        let auth_token = self.provider.validate_token(token).await?;
        
        if !auth_token.has_permission(&Permission::Publish) {
            return Err(StreamingError::AuthorizationFailed {
                user: auth_token.user_id.clone(),
                stream_name: stream_key.to_string(),
            });
        }

        let user = self.provider.get_user(&auth_token.user_id).await?
            .ok_or_else(|| StreamingError::AuthenticationFailed {
                stream_name: auth_token.user_id.clone(),
            })?;

        if !user.can_publish_to_stream(stream_key) {
            return Err(StreamingError::AuthorizationFailed {
                user: user.id,
                stream_name: stream_key.to_string(),
            });
        }

        Ok(user)
    }
}

// 全局认证提供者
use std::sync::OnceLock;
static GLOBAL_AUTH_PROVIDER: OnceLock<Arc<dyn AuthProvider>> = OnceLock::new();

pub fn get_global_auth_provider() -> Arc<dyn AuthProvider> {
    GLOBAL_AUTH_PROVIDER.get_or_init(|| {
        Arc::new(MemoryAuthProvider::default())
    }).clone()
}

pub fn get_auth_middleware() -> AuthMiddleware {
    AuthMiddleware::new(get_global_auth_provider())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_user_permissions() {
        let user = User::new("test".to_string(), "test".to_string())
            .with_permissions(vec![Permission::Publish, Permission::Subscribe])
            .with_stream_keys(vec!["stream1".to_string()]);

        assert!(user.has_permission(&Permission::Publish));
        assert!(user.has_permission(&Permission::Subscribe));
        assert!(!user.has_permission(&Permission::Admin));
        assert!(user.can_publish_to_stream("stream1"));
        assert!(!user.can_publish_to_stream("stream2"));
    }

    #[tokio::test]
    async fn test_token_expiration() {
        // 创建一个立即过期的令牌
        let token = AuthToken::new(
            "user1".to_string(),
            vec![Permission::Subscribe],
            Duration::from_millis(1), // 1毫秒后过期
        );

        // 等待足够长的时间确保过期
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(token.is_expired());

        // 测试未过期的令牌
        let valid_token = AuthToken::new(
            "user2".to_string(),
            vec![Permission::Subscribe],
            Duration::from_secs(3600), // 1小时后过期
        );
        assert!(!valid_token.is_expired());
    }

    #[tokio::test]
    async fn test_memory_auth_provider() {
        let provider = MemoryAuthProvider::new();
        
        let user = User::new("test_user".to_string(), "testuser".to_string())
            .with_permissions(vec![Permission::Publish]);
        
        provider.add_user("testuser".to_string(), "password123".to_string(), user).await.unwrap();
        
        // 测试认证
        let authenticated_user = provider.authenticate("testuser", "password123").await.unwrap();
        assert_eq!(authenticated_user.username, "testuser");
        
        // 测试错误密码
        assert!(provider.authenticate("testuser", "wrongpassword").await.is_err());
        
        // 测试令牌创建和验证
        let token = provider.create_token(&authenticated_user, Duration::from_secs(3600)).await.unwrap();
        let auth_token = provider.validate_token(&token).await.unwrap();
        assert_eq!(auth_token.user_id, "test_user");
    }
}
