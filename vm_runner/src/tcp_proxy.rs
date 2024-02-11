use std::sync::Arc;
use std::thread;

use std::io;
use std::net::{Shutdown, TcpStream};

pub(crate) fn splice(inc: TcpStream, out: TcpStream) {
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
        let a = lhs_arc.shutdown(Shutdown::Both);
        let b = rhs_arc.shutdown(Shutdown::Both);
        println!("{:?} {:?} done joining conn", a, b);
    }
}
