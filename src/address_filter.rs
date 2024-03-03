use ipnet::Ipv4Net;
use log::{debug, trace};
use std::collections::HashSet;
use std::net::SocketAddrV4;

pub struct AddressFilter {
    transmit_addresses_set: HashSet<SocketAddrV4>,
    block_nets: Vec<Ipv4Net>,
    allow_nets: Vec<Ipv4Net>,
}

impl AddressFilter {
    pub fn new(
        transmit_addresses_set: HashSet<SocketAddrV4>,
        block_nets: Vec<Ipv4Net>,
        allow_nets: Vec<Ipv4Net>,
    ) -> Self {
        // TODO: only log if non-zero?
        debug!("Blocking packets from {} subnets", block_nets.len());
        debug!("Allowing packets from {} subnets", allow_nets.len());
        Self {
            transmit_addresses_set,
            block_nets,
            allow_nets,
        }
    }

    pub fn should_transmit(&self, socket_addr: &SocketAddrV4) -> bool {
        let storm_check = self.transmit_addresses_set.contains(socket_addr);

        let in_block_net = self
            .block_nets
            .iter()
            .any(|net| net.contains(socket_addr.ip()));

        let in_allow_net = self
            .allow_nets
            .iter()
            .any(|net| net.contains(socket_addr.ip()));

        // TODO: Check semantics of this.
        let res = !storm_check && (!in_block_net || in_allow_net);

        if !res {
            trace!("Not transmitting packet from {:?}", socket_addr);
        }

        res
    }
}
