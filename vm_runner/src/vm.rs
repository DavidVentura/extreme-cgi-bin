use crate::tap;
use crate::tcp_proxy;
use std::error::Error;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;
use std::time::Instant;
use vmm::builder::{build_and_boot_microvm, StartMicrovmError};
use vmm::resources::VmResources;
use vmm::seccomp_filters::get_empty_filters;
use vmm::vmm_config::instance_info::{InstanceInfo, VmState};
use vmm::{EventManager, FcExitCode, HTTP_MAX_PAYLOAD_SIZE};

#[derive(Debug, thiserror::Error, displaydoc::Display)]
enum UtilsError {
    /// Failed to create VmResources: {0}
    CreateVmResources(vmm::resources::ResourcesError),
    /// Failed to build microVM: {0}
    BuildMicroVm(#[from] StartMicrovmError),
}

fn nth_ip_in_subnet(subnet: Ipv4Addr, n: u8) -> Ipv4Addr {
    let ip_oct = subnet.octets();
    Ipv4Addr::new(ip_oct[0], ip_oct[1], ip_oct[2], ip_oct[3] + n)
}

pub struct VmHandler {
    vms: Vec<VmNetCfg>,
    free: AtomicU64,
}

impl VmHandler {
    pub fn new(size: u8, subnet: Ipv4Addr) -> Result<VmHandler, Box<dyn Error>> {
        if size >= 64 {
            return Err("Up to 63 VMs per handler".into());
        }
        let mut bits: u64 = 0;
        for i in 0..size {
            bits |= 1 << i;
        }
        Ok(VmHandler {
            vms: VmHandler::populate_vm_configs(size as usize, subnet),
            free: AtomicU64::new(bits),
        })
    }
    pub fn handle_tcp_conn(&self, inc: TcpStream) -> Result<(), Box<dyn Error>> {
        let _free = self.free.load(Ordering::Relaxed);
        if _free == 0 {
            return Err("No free VMs to handle the request".into());
        }
        let first_idx = _free.trailing_zeros();
        self.free.fetch_xor(1 << first_idx, Ordering::Relaxed);
        let res = self.vms[first_idx as usize].handle_tcp_conn(inc);
        self.free.fetch_or(1 << first_idx, Ordering::Relaxed);
        res
    }

    fn populate_vm_configs(len: usize, subnet: Ipv4Addr) -> Vec<VmNetCfg> {
        // ip space runs out with this silly calculation for nth_ip_in_subnet
        assert!(len <= 63);
        let netmask = Ipv4Addr::new(255, 255, 255, 252);

        let mut ret = vec![];
        for j in 0..len {
            let tap_ip = nth_ip_in_subnet(subnet, (j as u8) * 4 + 0);
            let vm_ip = nth_ip_in_subnet(subnet, (j as u8) * 4 + 1);
            let vm_mac: Vec<u8> = vec![0x06, 0x00, 0xAC, 0x10, 0x0, j as u8];
            let tap_name = crate::tap::add_tap(j as u16, tap_ip, netmask)
                .expect("Failed to create a TAP device");

            let v = VmNetCfg {
                vm_ip,
                tap_ip,
                netmask,
                vm_mac,
                tap_iface: tap_name,
            };
            tap::register_vm_arp(&v).unwrap();
            ret.push(v);
        }

        println!("Sleeping 1.1s as the ARP cache gets wiped otherwise");
        thread::sleep(Duration::from_millis(1100));
        ret
    }
}

#[derive(Clone)]
pub(crate) struct VmNetCfg {
    pub(crate) vm_ip: Ipv4Addr,
    tap_ip: Ipv4Addr,
    netmask: Ipv4Addr,
    pub(crate) tap_iface: String,
    pub(crate) vm_mac: Vec<u8>,
}

impl VmNetCfg {
    pub(crate) fn handle_tcp_conn(&self, inc: TcpStream) -> Result<(), Box<dyn Error>> {
        let req_start = Instant::now();
        let clone = self.clone();

        thread::spawn(move || {
            let cstr = clone.connect(); // this blocks until the TCP conn dies
            tcp_proxy::splice(inc, cstr);
            println!("Request done in {:?}", req_start.elapsed());
        });
        self.make().expect("Could not create VM"); // this blocks until the VM dies
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
        let mac_str: Vec<String> = self
            .vm_mac
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
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
            mac_str.join(":"),
            self.tap_iface
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
