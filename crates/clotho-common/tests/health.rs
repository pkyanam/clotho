//! End-to-end health check: start a real gRPC server on an ephemeral port and
//! call Check over the wire.

use clotho_common::health::HealthService;
use clotho_common::pb::health::v1::{
    health_check_response::ServingStatus, health_client::HealthClient, HealthCheckRequest,
};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

#[tokio::test]
async fn health_check_round_trip() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(
        Server::builder()
            .add_service(HealthService::new("clotho-test", "0.1.0").into_server())
            .serve_with_incoming(TcpListenerStream::new(listener)),
    );

    let mut client = HealthClient::connect(format!("http://{addr}"))
        .await
        .expect("client connects");
    let response = client
        .check(HealthCheckRequest {
            service: String::new(),
        })
        .await
        .expect("check succeeds")
        .into_inner();

    assert_eq!(response.status, ServingStatus::Serving as i32);
    assert_eq!(response.service_name, "clotho-test");
    assert_eq!(response.version, "0.1.0");

    server.abort();
}
