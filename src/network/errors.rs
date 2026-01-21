use thiserror::Error;

// * Unified Error type for the Network Layer.
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Request failed: {0}")]
    Rquest(#[from] rquest::Error), // * CHANGED: Mapping from rquest::Error

    #[error("Soft Ban detected: {0}")]
    SoftBan(String),

    #[error("HTTP {0} Forbidden/Blocked")]
    HardBan(u16),

    #[error("Empty response body (< {0} bytes)")]
    EmptyResponse(usize),
    
    #[error("Invalid URL")]
    InvalidUrl,
}
