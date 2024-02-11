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
    //        .destination(gw)

    let mut t = tun::create(&config)?;
    t.persist()?;
    Ok(name)
}
