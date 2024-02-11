use crate::VmNetCfg;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
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
pub(crate) fn make_vm(cfg: VmNetCfg) -> Result<(), UtilsError> {
    let start = Instant::now();

    let instance_info = InstanceInfo {
        id: "anonymous-instance".to_string(),
        state: VmState::NotStarted,
        vmm_version: "Amazing version".to_string(),
        app_name: "cpu-template-helper".to_string(),
    };

    let boot_args = format!("panic=-1 reboot=t quiet ip.dev_wait_ms=0 root=/dev/vda ip={0}::{1}:{2}:hostname:eth0:off init=/init", cfg.vm_ip, cfg.tap_ip, cfg.netmask);
    let config = format!(
        r#"
{{
  "boot-source": {{
    "kernel_image_path": "/home/david/git/lk/vmlinux-mini-net",
    "boot_args": "{boot_args}"
  }},
  "machine-config": {{
    "vcpu_count": 1,
    "backed_by_hugepages": true,
    "mem_size_mib": 128
  }},
  "drives": [{{
    "drive_id": "rootfs",
    "path_on_host": "artifacts/rootfs.ext4",
    "is_root_device": true,
    "is_read_only": false
  }}],
  "network-interfaces": [{{
    "iface_id": "net0",
    "guest_mac": "{0}",
    "host_dev_name": "{1}"
  }}]
}}"#,
        cfg.vm_mac, cfg.tap_iface
    );
    let mut vm_resources =
        VmResources::from_json(&config, &instance_info, HTTP_MAX_PAYLOAD_SIZE, None)
            .map_err(UtilsError::CreateVmResources)?;
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
                return Ok(());
            }
            None => continue,
        }
    }
    Ok(())
}

pub(crate) fn connect_to_vm(ip: Ipv4Addr) -> TcpStream {
    let start = Instant::now();
    thread::sleep(Duration::from_millis(5));
    loop {
        let sr =
            TcpStream::connect_timeout(&SocketAddr::new(ip.into(), 8081), Duration::from_millis(1));
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
