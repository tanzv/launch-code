use std::net::TcpListener;

use thiserror::Error;

use crate::model::DebugConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDebugConfig {
    pub config: DebugConfig,
    pub requested_port: u16,
    pub fallback_applied: bool,
}

#[derive(Debug, Error)]
pub enum DebugError {
    #[error("invalid debug endpoint {host}:{port}: {source}")]
    InvalidEndpoint {
        host: String,
        port: u16,
        source: std::io::Error,
    },
    #[error("failed to allocate fallback debug port on {host}: {source}")]
    FallbackBindFailed {
        host: String,
        source: std::io::Error,
    },
}

pub fn resolve_debug_config(input: &DebugConfig) -> Result<ResolvedDebugConfig, DebugError> {
    let requested = input.port;
    let host = input.host.clone();

    match TcpListener::bind((host.as_str(), requested)) {
        Ok(listener) => {
            drop(listener);
            Ok(ResolvedDebugConfig {
                config: input.clone(),
                requested_port: requested,
                fallback_applied: false,
            })
        }
        Err(bind_error) => {
            let fallback = TcpListener::bind((host.as_str(), 0)).map_err(|source| {
                if bind_error.kind() == std::io::ErrorKind::InvalidInput {
                    DebugError::InvalidEndpoint {
                        host: host.clone(),
                        port: requested,
                        source: bind_error,
                    }
                } else {
                    DebugError::FallbackBindFailed {
                        host: host.clone(),
                        source,
                    }
                }
            })?;

            let fallback_port = fallback
                .local_addr()
                .map_err(|source| DebugError::FallbackBindFailed {
                    host: host.clone(),
                    source,
                })?
                .port();
            drop(fallback);

            Ok(ResolvedDebugConfig {
                config: DebugConfig {
                    host,
                    port: fallback_port,
                    wait_for_client: input.wait_for_client,
                    subprocess: input.subprocess,
                },
                requested_port: requested,
                fallback_applied: true,
            })
        }
    }
}
