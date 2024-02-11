mod tap;
mod tcp_proxy;
mod vm;

use vm::VmHandler;

use std::{
    net::{Ipv4Addr, TcpListener},
    thread,
};

fn main() {
    let listener = TcpListener::bind("0.0.0.0:8080").unwrap();
    let max_concurrent_vms: u8 = 20;
    let subnet = Ipv4Addr::new(172, 16, 0, 0);

    let handler = VmHandler::new(max_concurrent_vms, subnet).unwrap();

    thread::scope(|s| {
        for stream in listener.incoming() {
            match stream {
                Ok(inc) => {
                    s.spawn(|| match handler.handle_tcp_conn(inc) {
                        Err(e) => println!("Error creating VM: {:?}", e),
                        Ok(_) => (),
                    });
                }
                Err(e) => {
                    println!("Error with the stream: {:?}", e);
                }
            }
        }
    });
}
