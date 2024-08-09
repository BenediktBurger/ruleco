use std::{thread, time};
use ruleco;

fn main() {
    let publisher = ruleco::DataPublisher::new("pub".to_string(), "localhost", "11100");
    thread::sleep(time::Duration::from_millis(100));
    publisher.send_message("some message".as_bytes().to_vec());
    println!("Successfully finished");
}

