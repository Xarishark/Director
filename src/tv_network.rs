//! TV-reported network identities and direct Wake-on-LAN packet dispatch.
use serde::{Deserialize, Serialize};
use std::{
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MacAddress([u8; 6]);
impl TryFrom<String> for MacAddress {
    type Error = String;
    fn try_from(value: String) -> Result<Self, String> {
        value.parse()
    }
}
impl std::str::FromStr for MacAddress {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        let value = value.trim();
        let compact = if value.contains(':') || value.contains('-') {
            let separator = if value.contains(':') { ':' } else { '-' };
            let parts = value.split(separator).collect::<Vec<_>>();
            if parts.len() != 6 || parts.iter().any(|p| p.len() != 2) {
                return Err(
                    "Enter six pairs of hexadecimal digits, for example 00:11:22:33:44:55.".into(),
                );
            }
            parts.concat()
        } else {
            value.into()
        };
        if compact.len() != 12 || !compact.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Enter a valid MAC address, for example 00:11:22:33:44:55.".into());
        }
        let mut bytes = [0; 6];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = u8::from_str_radix(&compact[i * 2..i * 2 + 2], 16)
                .map_err(|_| "Invalid MAC address.")?;
        }
        if bytes == [0; 6] || bytes[0] & 1 != 0 {
            return Err("Use the TV adapter’s individual MAC address, not a broadcast or multicast address.".into());
        }
        Ok(Self(bytes))
    }
}
impl std::fmt::Display for MacAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}
impl From<MacAddress> for String {
    fn from(value: MacAddress) -> Self {
        value.to_string()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NetworkAddresses {
    pub wired: Option<MacAddress>,
    pub wifi: Option<MacAddress>,
    pub manual: Option<MacAddress>,
}
impl NetworkAddresses {
    pub fn targets(&self) -> Vec<MacAddress> {
        let mut result = vec![];
        for mac in [self.wired, self.wifi, self.manual].into_iter().flatten() {
            if !result.contains(&mac) {
                result.push(mac);
            }
        }
        result
    }
    /// A missing/refused read must not erase previously learned wake addresses.
    pub fn merge_learned(&mut self, learned: &Self) {
        if learned.wired.is_some() {
            self.wired = learned.wired;
        }
        if learned.wifi.is_some() {
            self.wifi = learned.wifi;
        }
    }
    pub fn summary(&self) -> String {
        [
            self.wired.map(|m| format!("Ethernet: {m}")),
            self.wifi.map(|m| format!("Wi-Fi: {m}")),
            self.manual.map(|m| format!("Manual: {m}")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
    }
}
pub fn magic_packet(mac: MacAddress) -> [u8; 102] {
    let mut packet = [0xff; 102];
    for bytes in packet[6..].as_chunks_mut::<6>().0 {
        bytes.copy_from_slice(&mac.0);
    }
    packet
}
#[derive(Clone, Debug)]
pub struct WakeReport {
    pub targets: usize,
    pub packets: usize,
}
fn dispatch(
    socket: &UdpSocket,
    destination: SocketAddrV4,
    targets: &[MacAddress],
) -> Result<WakeReport, String> {
    if targets.is_empty() {
        return Err("No wake address is saved. Edit the TV to read its network information or enter a MAC address.".into());
    }
    let mut packets = 0;
    for round in 0..3 {
        for mac in targets {
            let packet = magic_packet(*mac);
            let sent=socket.send_to(&packet,destination).map_err(|e|format!("Wake dispatch stopped after {packets} packets: {e}. A wake request may already have been sent."))?;
            if sent != packet.len() {
                return Err("Windows did not send a complete wake packet.".into());
            }
            packets += 1;
        }
        if round < 2 {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(WakeReport {
        targets: targets.len(),
        packets,
    })
}
/// Dispatch only: successful UDP transmission is not evidence that the panel woke.
pub fn wake(addresses: &NetworkAddresses) -> Result<WakeReport, String> {
    let targets = addresses.targets();
    if targets.is_empty() {
        return Err("No wake address is saved. Edit the TV to read its network information or enter a MAC address.".into());
    }
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .map_err(|e| format!("Could not open the wake connection: {e}"))?;
    socket.set_broadcast(true).map_err(|e| e.to_string())?;
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    dispatch(&socket, SocketAddrV4::new(Ipv4Addr::BROADCAST, 9), &targets)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_and_normalizes_network_addresses() {
        let mac: MacAddress = "00-11-22-aa-bb-cc".parse().unwrap();
        assert_eq!(mac.to_string(), "00:11:22:AA:BB:CC");
        for bad in [
            "00:00:00:00:00:00",
            "FF:FF:FF:FF:FF:FF",
            "01:11:22:33:44:55",
            "00:11:22:33:44:zz",
            "0:11:22:33:44:55",
            "garbage001122334455",
        ] {
            assert!(bad.parse::<MacAddress>().is_err());
        }
        let mut n = NetworkAddresses {
            wired: Some(mac),
            wifi: Some(mac),
            manual: None,
        };
        assert_eq!(n.targets().len(), 1);
        n.merge_learned(&NetworkAddresses::default());
        assert_eq!(n.wired, Some(mac));
    }
    #[test]
    fn wake_packet_is_exact_and_udp_dispatch_is_bounded() {
        let mac: MacAddress = "00:11:22:33:44:55".parse().unwrap();
        let packet = magic_packet(mac);
        assert_eq!(&packet[..6], &[255; 6]);
        for chunk in packet[6..].as_chunks::<6>().0 {
            assert_eq!(chunk, &[0, 17, 34, 51, 68, 85]);
        }
        let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let std::net::SocketAddr::V4(destination) = receiver.local_addr().unwrap() else {
            panic!()
        };
        let report = dispatch(&sender, destination, &[mac]).unwrap();
        assert_eq!(report.packets, 3);
        for _ in 0..3 {
            let mut data = [0; 103];
            let size = receiver.recv(&mut data).unwrap();
            assert_eq!(&data[..size], &packet);
        }
        assert!(dispatch(&sender, destination, &[]).is_err());
    }
}
