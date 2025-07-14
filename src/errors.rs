use thiserror::Error;
use hyper::{Body, Response, StatusCode};
use serde::Serialize;

/// 统一的流媒体服务错误类型
#[derive(Debug, Error)]
pub enum StreamingError {
    #[error("Stream not found: {stream_name}")]
    StreamNotFound { stream_name: String },
    
    #[error("Authentication failed for stream: {stream_name}")]
    AuthenticationFailed { stream_name: String },
    
    #[error("Authorization failed for user {user} on stream {stream_name}")]
    AuthorizationFailed { user: String, stream_name: String },
    
    #[error("Protocol error: {message}")]
    ProtocolError { message: String },
    
    #[error("Storage error: {source}")]
    StorageError { 
        #[from] 
        source: std::io::Error 
    },
    
    #[error("Configuration error: {message}")]
    ConfigError { message: String },
    
    #[error("Network error: {message}")]
    NetworkError { message: String },
    
    #[error("Codec error: {message}")]
    CodecError { message: String },

    #[error("GOP processing error: {message}")]
    GopError { message: String },

    #[error("Video encoding error: {message}")]
    VideoEncodingError { message: String },

    #[error("Rate limit exceeded for {identifier}")]
    RateLimitExceeded { identifier: String },
    
    #[error("Resource exhausted: {resource}")]
    ResourceExhausted { resource: String },
    
    #[error("Invalid request: {message}")]
    InvalidRequest { message: String },
    
    #[error("Service unavailable: {message}")]
    ServiceUnavailable { message: String },
    
    #[error("Internal server error: {message}")]
    InternalError { message: String },
}

impl From<config::ConfigError> for StreamingError {
    fn from(err: config::ConfigError) -> Self {
        StreamingError::ConfigError {
            message: err.to_string(),
        }
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for StreamingError {
    fn from(err: tokio::sync::mpsc::error::SendError<T>) -> Self {
        StreamingError::InternalError {
            message: format!("Channel send error: {}", err),
        }
    }
}

impl StreamingError {
    pub fn error_code(&self) -> &'static str {
        match self {
            StreamingError::StreamNotFound { .. } => "STREAM_NOT_FOUND",
            StreamingError::AuthenticationFailed { .. } => "AUTH_FAILED",
            StreamingError::AuthorizationFailed { .. } => "AUTHZ_FAILED",
            StreamingError::ProtocolError { .. } => "PROTOCOL_ERROR",
            StreamingError::StorageError { .. } => "STORAGE_ERROR",
            StreamingError::ConfigError { .. } => "CONFIG_ERROR",
            StreamingError::NetworkError { .. } => "NETWORK_ERROR",
            StreamingError::CodecError { .. } => "CODEC_ERROR",
            StreamingError::GopError { .. } => "GOP_ERROR",
            StreamingError::VideoEncodingError { .. } => "VIDEO_ENCODING_ERROR",
            StreamingError::RateLimitExceeded { .. } => "RATE_LIMIT_EXCEEDED",
            StreamingError::ResourceExhausted { .. } => "RESOURCE_EXHAUSTED",
            StreamingError::InvalidRequest { .. } => "INVALID_REQUEST",
            StreamingError::ServiceUnavailable { .. } => "SERVICE_UNAVAILABLE",
            StreamingError::InternalError { .. } => "INTERNAL_ERROR",
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self {
            StreamingError::StreamNotFound { .. } => StatusCode::NOT_FOUND,
            StreamingError::AuthenticationFailed { .. } => StatusCode::UNAUTHORIZED,
            StreamingError::AuthorizationFailed { .. } => StatusCode::FORBIDDEN,
            StreamingError::ProtocolError { .. } => StatusCode::BAD_REQUEST,
            StreamingError::StorageError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            StreamingError::ConfigError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            StreamingError::NetworkError { .. } => StatusCode::BAD_GATEWAY,
            StreamingError::CodecError { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            StreamingError::GopError { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            StreamingError::VideoEncodingError { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            StreamingError::RateLimitExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
            StreamingError::ResourceExhausted { .. } => StatusCode::SERVICE_UNAVAILABLE,
            StreamingError::InvalidRequest { .. } => StatusCode::BAD_REQUEST,
            StreamingError::ServiceUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            StreamingError::InternalError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, 
            StreamingError::NetworkError { .. } |
            StreamingError::ServiceUnavailable { .. } |
            StreamingError::ResourceExhausted { .. }
        )
    }

    pub fn should_log_error(&self) -> bool {
        !matches!(self,
            StreamingError::StreamNotFound { .. } |
            StreamingError::AuthenticationFailed { .. } |
            StreamingError::AuthorizationFailed { .. } |
            StreamingError::RateLimitExceeded { .. }
        )
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
    pub message: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    pub fn from_error(error: &StreamingError) -> Self {
        Self {
            error: "StreamingError".to_string(),
            code: error.error_code().to_string(),
            message: error.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// 错误处理器 - 统一处理错误并生成HTTP响应
pub struct ErrorHandler;

impl ErrorHandler {
    pub fn handle_error(error: &StreamingError) -> Response<Body> {
        // 根据错误类型决定是否记录日志
        if error.should_log_error() {
            log::error!("Streaming error: {}", error);
        } else {
            log::warn!("Client error: {}", error);
        }

        let error_response = ErrorResponse::from_error(error);
        let status = error.http_status();
        
        // 对于某些错误类型，添加额外的响应头
        let mut response = Response::builder()
            .status(status)
            .header("Content-Type", "application/json");

        // 添加CORS头
        response = response
            .header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
            .header("Access-Control-Allow-Headers", "Content-Type, Authorization");

        // 对于速率限制错误，添加Retry-After头
        if matches!(error, StreamingError::RateLimitExceeded { .. }) {
            response = response.header("Retry-After", "60");
        }

        let body = match serde_json::to_string(&error_response) {
            Ok(json) => Body::from(json),
            Err(_) => Body::from(r#"{"error":"InternalError","message":"Failed to serialize error response"}"#),
        };

        response.body(body).unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Failed to build error response"))
                .unwrap()
        })
    }

    pub fn handle_success<T: Serialize>(data: T) -> Response<Body> {
        let response = match serde_json::to_string(&data) {
            Ok(json) => Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(Body::from(json)),
            Err(e) => {
                log::error!("Failed to serialize success response: {}", e);
                let error = StreamingError::InternalError {
                    message: "Failed to serialize response".to_string(),
                };
                return Self::handle_error(&error);
            }
        };

        response.unwrap_or_else(|_| {
            let error = StreamingError::InternalError {
                message: "Failed to build success response".to_string(),
            };
            Self::handle_error(&error)
        })
    }
}

/// Result类型别名
pub type Result<T> = std::result::Result<T, StreamingError>;

/// 便利宏用于创建特定类型的错误
#[macro_export]
macro_rules! streaming_error {
    (stream_not_found, $stream:expr) => {
        $crate::errors::StreamingError::StreamNotFound {
            stream_name: $stream.to_string(),
        }
    };
    (auth_failed, $stream:expr) => {
        $crate::errors::StreamingError::AuthenticationFailed {
            stream_name: $stream.to_string(),
        }
    };
    (protocol_error, $msg:expr) => {
        $crate::errors::StreamingError::ProtocolError {
            message: $msg.to_string(),
        }
    };
    (config_error, $msg:expr) => {
        $crate::errors::StreamingError::ConfigError {
            message: $msg.to_string(),
        }
    };
    (internal_error, $msg:expr) => {
        $crate::errors::StreamingError::InternalError {
            message: $msg.to_string(),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        let error = StreamingError::StreamNotFound {
            stream_name: "test".to_string(),
        };
        assert_eq!(error.error_code(), "STREAM_NOT_FOUND");
        assert_eq!(error.http_status(), StatusCode::NOT_FOUND);
        assert!(!error.is_retryable());
        assert!(!error.should_log_error());
    }

    #[test]
    fn test_error_response_serialization() {
        let error = StreamingError::AuthenticationFailed {
            stream_name: "test_stream".to_string(),
        };
        let response = ErrorResponse::from_error(&error);
        
        assert_eq!(response.code, "AUTH_FAILED");
        assert!(response.message.contains("test_stream"));
        assert!(response.details.is_none());
    }

    #[test]
    fn test_retryable_errors() {
        let network_error = StreamingError::NetworkError {
            message: "Connection failed".to_string(),
        };
        assert!(network_error.is_retryable());

        let auth_error = StreamingError::AuthenticationFailed {
            stream_name: "test".to_string(),
        };
        assert!(!auth_error.is_retryable());
    }
}
