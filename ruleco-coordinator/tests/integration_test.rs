use ruleco_coordinator::app::CoordinatorApp;
use std::thread;
use std::time::Duration;
use zmq;

fn find_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

#[test]
#[ignore]
fn test_coordinator_sign_in() {
    let port = find_free_port();
    let namespace = "test_namespace_sign_in";

    let mut coordinator =
        CoordinatorApp::new(namespace, Some(port), None).expect("Failed to create coordinator");

    let _coordinator_thread = thread::spawn(move || {
        coordinator.run().expect("Coordinator failed to run");
    });

    thread::sleep(Duration::from_millis(100));

    let context = zmq::Context::new();
    let client_socket = context.socket(zmq::DEALER).unwrap();
    client_socket
        .connect(&format!("tcp://127.0.0.1:{}", port))
        .expect("Failed to connect client");

    let sign_in_request = r#"{"jsonrpc":"2.0","method":"sign_in","id":1}"#;

    client_socket
        .send(sign_in_request, 0)
        .expect("Failed to send request");

    let response = client_socket
        .recv_string(0)
        .expect("Failed to receive response")
        .unwrap();

    assert!(response.contains(r#""result":null"#));
}
