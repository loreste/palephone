use thiserror::Error;

#[derive(Debug, Error)]
pub enum PaleError {
    #[error("PJSIP error: {0} (status={1})")]
    Pjsip(String, i32),

    #[error("Engine not initialized")]
    NotInitialized,

    #[error("Engine already running")]
    AlreadyRunning,

    #[error("Invalid account configuration: {0}")]
    InvalidConfig(String),

    #[error("Call not found: {0}")]
    CallNotFound(i32),

    #[error("Account not found: {0}")]
    AccountNotFound(i32),

    #[error("Channel send error: {0}")]
    ChannelSend(String),

    #[error("Thread error: {0}")]
    Thread(String),
}

pub type PaleResult<T> = Result<T, PaleError>;
