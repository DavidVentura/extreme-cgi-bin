mod tap;
mod tcp_proxy;
mod vm;

use lazy_static::lazy_static; // 1.4.0
use std::sync::Mutex;

use std::net::{Ipv4Addr, TcpListener};
use std::thread;

use std::time::Instant;

#[derive(Clone)]
pub(crate) struct VmNetCfg {
    vm_ip: Ipv4Addr,
    tap_ip: Ipv4Addr,
    netmask: Ipv4Addr,
    tap_iface: String,
    vm_mac: String,
}

fn first_free_vm_idx(max: usize, free: &[bool]) -> Option<usize> {
    for i in 0..max {
        if free[i] {
            return Some(i);
        }
    }
    None
}

lazy_static! {
    static ref VM_FREE: Mutex<Vec<bool>> = Mutex::new(vec![]);
}

fn main() {
    let listener = TcpListener::bind("0.0.0.0:8080").unwrap();
    let max_concurrent_vms: usize = 20;
    let subnet = Ipv4Addr::new(172, 16, 0, 0);
    let netmask = Ipv4Addr::new(255, 255, 255, 252);
    let gw_oct = subnet.octets();
    let ip_oct = subnet.octets();

    let mut vm_net_cfg: Vec<VmNetCfg> = Vec::new();
    for j in 0..max_concurrent_vms {
        let tap_ip = Ipv4Addr::new(
            ip_oct[0],
            ip_oct[1],
            ip_oct[2],
            ip_oct[3] + (j as u8) * 4 + 0,
        );
        let vm_ip = Ipv4Addr::new(
            gw_oct[0],
            gw_oct[1],
            gw_oct[2],
            gw_oct[3] + (j as u8) * 4 + 1,
        );
        let tap_name =
            tap::add_tap(j as u16, tap_ip, netmask).expect("Failed to create a TAP device");

        vm_net_cfg.push(VmNetCfg {
            vm_ip,
            tap_ip,
            netmask,
            tap_iface: tap_name,
            vm_mac: format!("06:00:AC:10:00:{j:02x}"),
        });
        VM_FREE.lock().unwrap().push(true);
    }

    for stream in listener.incoming() {
        match stream {
            Ok(inc) => {
                let req_start = Instant::now();

                let free_vm_idx =
                    match first_free_vm_idx(max_concurrent_vms, &VM_FREE.lock().unwrap()) {
                        None => {
                            println!("no free vms");
                            continue;
                        }
                        Some(i) => i,
                    };
                let this_vm_cfg = vm_net_cfg[free_vm_idx].clone();
                let this_vm_cfg2 = this_vm_cfg.clone();

                VM_FREE.lock().unwrap()[free_vm_idx] = false;

                let threads = vec![
                    thread::spawn(move || {
                        vm::make_vm(this_vm_cfg).unwrap();
                    }),
                    thread::spawn(move || {
                        let cstr = vm::connect_to_vm(this_vm_cfg2.vm_ip);
                        tcp_proxy::splice(inc, cstr);
                        println!("Request done in {:?}", req_start.elapsed());
                    }),
                ];

                thread::spawn(move || {
                    for t in threads {
                        t.join().unwrap();
                    }
                    VM_FREE.lock().unwrap()[free_vm_idx] = true;
                });
            }
            Err(_) => {
                println!("Error");
            }
        }
    }
}
