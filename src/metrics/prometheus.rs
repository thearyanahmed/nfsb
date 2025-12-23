use anyhow::Result;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use http_body_util::Full;
use prometheus::{Encoder, TextEncoder};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

use super::Collector;

/// Handle for the running Prometheus server
pub struct ServerHandle {
    shutdown_tx: oneshot::Sender<()>,
    join_handle: JoinHandle<()>,
}

impl ServerHandle {
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(());
        let _ = self.join_handle.await;
    }
}

/// Start the Prometheus metrics HTTP server
pub async fn start_server(port: u16) -> Result<ServerHandle> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;

    info!(port = port, "Prometheus metrics server listening");

    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

    let join_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _)) => {
                            let io = TokioIo::new(stream);

                            tokio::spawn(async move {
                                let service = service_fn(handle_request);

                                if let Err(err) = http1::Builder::new()
                                    .serve_connection(io, service)
                                    .await
                                {
                                    error!(error = %err, "Error serving connection");
                                }
                            });
                        }
                        Err(e) => {
                            error!(error = %e, "Error accepting connection");
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    info!("Shutting down Prometheus server");
                    break;
                }
            }
        }
    });

    Ok(ServerHandle {
        shutdown_tx,
        join_handle,
    })
}

async fn handle_request(
    req: Request<Incoming>,
) -> Result<Response<Full<bytes::Bytes>>, Infallible> {
    let response = match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
            let encoder = TextEncoder::new();
            let metric_families = prometheus::gather();
            let mut buffer = Vec::new();

            match encoder.encode(&metric_families, &mut buffer) {
                Ok(_) => Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", encoder.format_type())
                    .body(Full::new(bytes::Bytes::from(buffer)))
                    .unwrap(),
                Err(e) => {
                    error!(error = %e, "Failed to encode metrics");
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Full::new(bytes::Bytes::from("Failed to encode metrics")))
                        .unwrap()
                }
            }
        }
        (&Method::GET, "/health") | (&Method::GET, "/") => Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(bytes::Bytes::from("OK")))
            .unwrap(),
        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(bytes::Bytes::from("Not Found")))
            .unwrap(),
    };

    debug!(
        method = %req.method(),
        path = req.uri().path(),
        status = %response.status(),
        "Handled request"
    );

    Ok(response)
}
