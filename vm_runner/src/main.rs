mod tap;
mod tcp_proxy;
mod vm;

use vm::VmNetCfg;

use std::net::{Ipv4Addr, TcpListener};

fn main() {
    let listener = TcpListener::bind("0.0.0.0:8080").unwrap();
    let max_concurrent_vms: usize = 20;
    let subnet = Ipv4Addr::new(172, 16, 0, 0);
    vm::populate_vm_configs(max_concurrent_vms, subnet);

    for stream in listener.incoming() {
        match stream {
            Ok(inc) => match VmNetCfg::handle_tcp_conn(inc) {
                Err(e) => println!("Error creating VM: {:?}", e),
                Ok(_) => (),
            },
            Err(e) => {
                println!("Error with the stream: {:?}", e);
            }
        }
    }
}
