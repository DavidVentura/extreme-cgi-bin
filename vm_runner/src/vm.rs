use crate::tcp_proxy;
use lazy_static::lazy_static;
use std::error::Error;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::Mutex;
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

lazy_static! {
    // How do you do this properly???
    static ref VMS: Mutex<Vec<VmNetCfg>> = Mutex::new(vec![]);
}

fn first_free_vm_idx(vms: &[VmNetCfg]) -> Option<usize> {
    for i in 0..vms.len() {
        if vms[i].free {
            return Some(i);
        }
    }
    None
}
fn nth_ip_in_subnet(subnet: Ipv4Addr, n: u8) -> Ipv4Addr {
    let ip_oct = subnet.octets();
    Ipv4Addr::new(ip_oct[0], ip_oct[1], ip_oct[2], ip_oct[3] + n)
}

pub(crate) fn populate_vm_configs(len: usize, subnet: Ipv4Addr) {
    let netmask = Ipv4Addr::new(255, 255, 255, 252);

    for j in 0..len {
        let tap_ip = nth_ip_in_subnet(subnet, (j as u8) * 4 + 0);
        let vm_ip = nth_ip_in_subnet(subnet, (j as u8) * 4 + 1);
        let vm_mac = format!("06:00:AC:10:00:{j:02x}");
        let tap_name =
            crate::tap::add_tap(j as u16, tap_ip, netmask).expect("Failed to create a TAP device");

        VMS.lock().unwrap().push(VmNetCfg {
            vm_ip,
            tap_ip,
            netmask,
            vm_mac,
            tap_iface: tap_name,
            free: true,
        });
    }
}
#[derive(Clone)]
pub(crate) struct VmNetCfg {
    vm_ip: Ipv4Addr,
    tap_ip: Ipv4Addr,
    netmask: Ipv4Addr,
    tap_iface: String,
    vm_mac: String,
    free: bool,
}

impl VmNetCfg {
    pub(crate) fn handle_tcp_conn(inc: TcpStream) -> Result<(), Box<dyn Error>> {
        let idx = match first_free_vm_idx(&VMS.lock().unwrap()) {
            None => {
                return Err("no free vms, no request for you".into());
            }
            Some(i) => i,
        };
        let req_start = Instant::now();
        let this = VMS.lock().unwrap()[idx].clone();
        println!("Am handling as vmm {}", idx);
        VMS.lock().unwrap()[idx].free = false;

        let clone = this.clone();
        thread::spawn(move || {
            this.make().expect("Could not create VM");
            VMS.lock().unwrap()[idx].free = true;
        });

        thread::spawn(move || {
            let cstr = clone.connect();
            tcp_proxy::splice(inc, cstr);
            println!("Request done in {:?}, idx: {}", req_start.elapsed(), idx);
        });
        Ok(())
    }

    fn connect(&self) -> TcpStream {
        let start = Instant::now();
        thread::sleep(Duration::from_millis(5));
        loop {
            let sr = TcpStream::connect_timeout(
                &SocketAddr::new(self.vm_ip.into(), 8081),
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

    fn make(&self) -> Result<(), UtilsError> {
        let start = Instant::now();

        let instance_info = InstanceInfo {
            id: "anonymous-instance".to_string(),
            state: VmState::NotStarted,
            vmm_version: "Amazing version".to_string(),
            app_name: "cpu-template-helper".to_string(),
        };

        let boot_args = format!("panic=-1 reboot=t quiet ip.dev_wait_ms=0 root=/dev/vda ip={0}::{1}:{2}:hostname:eth0:off init=/init", self.vm_ip, self.tap_ip, self.netmask);
        // TODO: figure out how to pass a real config and not json :^)
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
    "mem_size_mib": 32
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
            self.vm_mac, self.tap_iface
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
        )?;
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
}
