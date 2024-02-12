use crate::vm::VmNetCfg;
use libc;
use std::io;
use std::net::Ipv4Addr;
use tun;
use tun::Layer;

pub(crate) fn add_tap(id: u16, ip: Ipv4Addr, netmask: Ipv4Addr) -> Result<String, tun::Error> {
    let name = format!("mytap{id}");
    let mut config = tun::Configuration::default();
    config
        .address(ip)
        .netmask(netmask)
        .layer(Layer::L2)
        .name(name.clone())
        .up();

    let mut t = tun::create(&config)?;
    t.persist()?;
    Ok(name)
}

pub(crate) fn register_vm_arp(cfg: &VmNetCfg) -> Result<(), io::Error> {
    let ipo = cfg.vm_ip.octets();

    let chaddr: &mut [libc::c_char] = &mut [0; 6];
    for i in 0..6 {
        chaddr[i] = cfg.vm_mac[i] as libc::c_char;
    }

    let mut iface_name: [libc::c_char; 16] = [0; 16];
    let _b = cfg.tap_iface.as_bytes();
    for i in 0.._b.len() {
        iface_name[i] = _b[i] as i8;
    }

    // create arp_ha (for hardware addr)
    let arp_ha: libc::sockaddr = libc::sockaddr {
        sa_family: libc::ARPHRD_ETHER,
        sa_data: unsafe {
            let mut sa_data = [0; 14];
            let len = 6; // mac len
            sa_data[..len].copy_from_slice(std::mem::transmute::<_, &[libc::c_char]>(chaddr));
            sa_data
        },
    };

    let mut addr_in: libc::sockaddr_in = libc::sockaddr_in {
        sin_family: libc::AF_INET as _,
        ..unsafe { std::mem::zeroed() }
    };
    addr_in.sin_addr.s_addr = u32::from_le_bytes(ipo);

    // memcpy to sockaddr for arp_req
    let arp_pa: libc::sockaddr = unsafe { std::mem::transmute(addr_in) };
    let arp_req = libc::arpreq {
        arp_pa,
        arp_ha,
        arp_flags: libc::ATF_COM | libc::ATF_PERM,
        arp_dev: iface_name,
        ..unsafe { std::mem::zeroed() }
    };

    let soc = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
    let res = unsafe { libc::ioctl(soc, libc::SIOCSARP, &arp_req as *const libc::arpreq) };
    if res == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
