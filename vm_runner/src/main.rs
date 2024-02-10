use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use std::time::Instant;
use vmm::builder::{build_and_boot_microvm, StartMicrovmError};
use vmm::resources::VmResources;
use vmm::seccomp_filters::get_empty_filters;
use vmm::vmm_config::instance_info::{InstanceInfo, VmState};
use vmm::{EventManager, FcExitCode, HTTP_MAX_PAYLOAD_SIZE};

#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum UtilsError {
    /// Failed to create VmResources: {0}
    CreateVmResources(vmm::resources::ResourcesError),
    /// Failed to build microVM: {0}
    BuildMicroVm(#[from] StartMicrovmError),
}

fn make_vm() {
    let start = Instant::now();

    let instance_info = InstanceInfo {
        id: "anonymous-instance".to_string(),
        state: VmState::NotStarted,
        vmm_version: "Amazing version".to_string(),
        app_name: "cpu-template-helper".to_string(),
    };

    // TODO parametrize
    let config = r#"
{
  "boot-source": {
    "kernel_image_path": "/home/david/git/lk/vmlinux-mini-net",
    "boot_args": "panic=-1 reboot=t quiet ip.dev_wait_ms=0 root=/dev/vda ip=172.16.0.2::172.16.0.1:255.255.255.0:hostname:eth0:off init=/goinit"
  },
  "machine-config": {
    "vcpu_count": 1,
    "backed_by_hugepages": true,
    "mem_size_mib": 128
  },
  "drives": [{
    "drive_id": "rootfs",
    "path_on_host": "/home/david/git/lk/rootfs.ext4",
    "is_root_device": true,
    "is_read_only": false
  }],
  "network-interfaces": [{
    "iface_id": "net1",
    "guest_mac": "06:00:AC:10:00:02",
    "host_dev_name": "tap0"
  }]
}
        "#;
    let mut vm_resources =
        VmResources::from_json(config, &instance_info, HTTP_MAX_PAYLOAD_SIZE, None)
            .map_err(UtilsError::CreateVmResources)
            .unwrap();
    vm_resources.boot_timer = false;
    let mut event_manager = EventManager::new().unwrap();
    let seccomp_filters = get_empty_filters();

    let vm = build_and_boot_microvm(
        &instance_info,
        &vm_resources,
        &mut event_manager,
        &seccomp_filters,
    )
    .unwrap();
    let elapsed = start.elapsed();
    println!("Time to start VM: {:?}", elapsed);
    loop {
        event_manager.run().unwrap();
        match vm.lock().unwrap().shutdown_exit_code() {
            Some(FcExitCode::Ok) => break,
            Some(_) => {
                println!("vm died??");
                return;
            }
            None => continue,
        }
    }
}

fn connect_to_vm() -> TcpStream {
    let start = Instant::now();
    thread::sleep(Duration::from_millis(5));
    loop {
        let sr = TcpStream::connect_timeout(
            &SocketAddr::from(([172, 16, 0, 2], 8081)),
            Duration::from_millis(1),
        );
        match sr {
            Ok(sr) => {
                println!("is up - {:?}", start.elapsed());
                return sr;
            }
            Err(_) => {
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

fn splice(inc: TcpStream, out: TcpStream) {
    // TODO benchmark, does this achieve anything
    inc.set_nodelay(true).unwrap();
    out.set_nodelay(true).unwrap();

    let lhs_arc = Arc::new(inc);
    let rhs_arc = Arc::new(out);

    // TODO This should be a splice
    let (mut lhs_tx, mut lhs_rx) = (lhs_arc.try_clone().unwrap(), lhs_arc.try_clone().unwrap());
    let (mut rhs_tx, mut rhs_rx) = (rhs_arc.try_clone().unwrap(), rhs_arc.try_clone().unwrap());

    let connections = vec![
        thread::spawn(move || io::copy(&mut lhs_tx, &mut rhs_rx).unwrap()),
        thread::spawn(move || io::copy(&mut rhs_tx, &mut lhs_rx).unwrap()),
    ];

    for t in connections {
        println!("joining 1 conn");
        t.join().unwrap();
        println!("done joining conn");
    }
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:8080").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(inc) => {
                thread::spawn(move || {
                    make_vm();
                });

                thread::spawn(move || {
                    let cstr = connect_to_vm();
                    splice(inc, cstr);
                });
            }
            Err(_) => {
                println!("Error");
            }
        }
    }
}
