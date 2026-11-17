//! Server query, port checks and the addresses friends can use to join.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Splits a `getInfo` answer (`....infoResponse\n\key\value\key\value`) into pairs.
pub fn parse_info(response: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(start) = response.find('\\') else {
        return out;
    };
    let body = response[start + 1..].trim_end_matches(['\0', '\r', '\n']);
    let parts: Vec<&str> = body.split('\\').collect();
    for pair in parts.chunks(2) {
        if let [k, v] = pair {
            out.insert((*k).to_string(), (*v).to_string());
        }
    }
    out
}

/// Asks a server for its info. None if nothing valid answers within `timeout`.
pub fn query(addr: IpAddr, port: u16, timeout: Duration) -> Option<BTreeMap<String, String>> {
    let bind: SocketAddr = match addr {
        IpAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
        IpAddr::V6(_) => (std::net::Ipv6Addr::UNSPECIFIED, 0).into(),
    };
    let sock = UdpSocket::bind(bind).ok()?;
    sock.set_read_timeout(Some(timeout)).ok()?;
    let challenge =
        format!("bsm{:x}", SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let mut packet = vec![0xFFu8; 4];
    packet.extend_from_slice(format!("getInfo {challenge}").as_bytes());
    sock.send_to(&packet, (addr, port)).ok()?;
    let mut buf = [0u8; 4096];
    let (n, _) = sock.recv_from(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf[..n]).to_string();
    if !text.contains("infoResponse") || !text.contains(&challenge) {
        return None;
    }
    Some(parse_info(&text))
}

/// True if nothing on this PC is using the UDP port.
pub fn port_free(port: u16) -> bool {
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok()
}

/// First free UDP port at or after `start`.
pub fn next_free_port(start: u16) -> Option<u16> {
    (start.max(1024)..=65535).find(|p| port_free(*p))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddrKind {
    /// Home or office network: works for players on the same network.
    Lan,
    /// Radmin VPN: works for friends in the same Radmin network.
    Radmin,
    /// 100.64.0.0/10: carrier-grade NAT or a mesh VPN such as Tailscale.
    SharedNat,
    /// Looks like a real internet address on this PC.
    Public,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Addr {
    pub adapter: String,
    pub ip: Ipv4Addr,
    pub kind: AddrKind,
}

pub fn classify(adapter: &str, ip: Ipv4Addr) -> Option<AddrKind> {
    let o = ip.octets();
    if ip.is_loopback() || ip.is_unspecified() || ip.is_link_local() {
        return None;
    }
    if adapter.to_lowercase().contains("radmin") || o[0] == 26 {
        return Some(AddrKind::Radmin);
    }
    if ip.is_private() {
        return Some(AddrKind::Lan);
    }
    if o[0] == 100 && (64..128).contains(&o[1]) {
        return Some(AddrKind::SharedNat);
    }
    Some(AddrKind::Public)
}

/// IPv4 addresses of this PC that another player could connect to.
pub fn addresses() -> Vec<Addr> {
    let mut out: Vec<Addr> = Vec::new();
    if let Ok(list) = local_ip_address::list_afinet_netifas() {
        for (adapter, ip) in list {
            if let IpAddr::V4(v4) = ip {
                if let Some(kind) = classify(&adapter, v4) {
                    if !out.iter().any(|a| a.ip == v4) {
                        out.push(Addr { adapter, ip: v4, kind });
                    }
                }
            }
        }
    }
    out
}

/// Text a player pastes into the BOIII console (`~` key).
pub fn connect_command(ip: &str, port: u16, password: &str) -> String {
    if password.is_empty() {
        format!("connect {ip}:{port}")
    } else {
        format!("password {password}; connect {ip}:{port}")
    }
}

/// Console command that puts the mod menu on the arrow keys. The game script
/// can only see the four action-slot buttons (D-pad, or keys 1 2 3 4), so the
/// arrow keys are bound to those. Typed once per player; the game remembers it.
pub const ARROW_KEYS_COMMAND: &str = "bind UPARROW \"+actionslot 1\"; bind DOWNARROW \"+actionslot 2\"; bind LEFTARROW \"+actionslot 3\"; bind RIGHTARROW \"+actionslot 4\"";

/// Address to hand out first: Radmin if present (works across the internet),
/// otherwise the LAN address.
pub fn preferred(addrs: &[Addr]) -> Option<&Addr> {
    addrs
        .iter()
        .find(|a| a.kind == AddrKind::Radmin)
        .or_else(|| addrs.iter().find(|a| a.kind == AddrKind::Lan))
        .or_else(|| addrs.first())
}

/// Plain-language explanation of how friends reach the server. The tool does
/// not change the firewall or the router; it tells the user what to allow.
pub fn join_help(port: u16, password: &str, addrs: &[Addr]) -> String {
    let mut s = String::new();
    s.push_str("HOW FRIENDS JOIN\r\n\r\n");
    s.push_str("In BOIII they open the console with the ~ key and type one of these:\r\n\r\n");
    if addrs.is_empty() {
        s.push_str("  (no network address found on this PC)\r\n");
    }
    for a in addrs {
        let note = match a.kind {
            AddrKind::Lan => "same home network (LAN)",
            AddrKind::Radmin => "friends in your Radmin VPN network",
            AddrKind::SharedNat => "shared/VPN address (Tailscale or provider NAT)",
            AddrKind::Public => "public internet address of this PC",
        };
        s.push_str(&format!(
            "  {}    <- {} [{}]\r\n",
            connect_command(&a.ip.to_string(), port, password),
            note,
            a.adapter
        ));
    }
    s.push_str(&format!("  {}    <- from this same PC\r\n", connect_command("127.0.0.1", port, password)));
    s.push_str(&format!(
        "\r\nWINDOWS FIREWALL\r\nThe first time the server starts, Windows may ask whether boiii.exe may \
         communicate on the network. Choose Allow (private networks, and public if you use a VPN adapter). \
         If you never saw the question and friends time out, add an inbound rule yourself: \
         Windows Security > Firewall > Advanced settings > Inbound Rules > New Rule > Port > UDP {port} > Allow. \
         This tool does not change firewall settings.\r\n"
    ));
    s.push_str(&format!(
        "\r\nOVER THE INTERNET WITHOUT A VPN\r\nForward UDP port {port} in your router to this PC, then friends use \
         your public IP address. This cannot work when the provider shares one public address between customers \
         (carrier-grade NAT, common on 4G/5G routers): the address shown in the router's WAN status then starts \
         with 100.64-100.127 or 10., or differs from what a \"what is my IP\" website shows. In that case use \
         Radmin VPN (everyone joins the same Radmin network and connects to the 26.x.x.x address above) or a \
         UDP tunnel service such as playit.gg.\r\n"
    ));
    s.push_str(&format!(
        "\r\nPLAYING ON THIS PC TOO\r\nThe game client needs its own port. The server uses {port}; start your game \
         normally and it will pick another one (usually {}).\r\n",
        port.saturating_add(1)
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_info_response() {
        let r = "\u{fffd}\u{fffd}\u{fffd}\u{fffd}infoResponse\n\\challenge\\abc\\mapname\\zm_zod\\clients\\0\\sv_maxclients\\4\\hostname\\My Server\n";
        let m = parse_info(r);
        assert_eq!(m.get("mapname").map(String::as_str), Some("zm_zod"));
        assert_eq!(m.get("hostname").map(String::as_str), Some("My Server"));
        assert_eq!(m.len(), 5);
        assert!(parse_info("no backslashes").is_empty());
    }

    #[test]
    fn query_talks_to_a_fake_server_and_port_checks_work() {
        let server = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = server.local_addr().unwrap().port();
        let t = std::thread::spawn(move || {
            let mut buf = [0u8; 512];
            let (n, from) = server.recv_from(&mut buf).unwrap();
            assert_eq!(&buf[..4], &[0xFF; 4]);
            let req = String::from_utf8_lossy(&buf[4..n]).to_string();
            let challenge = req.strip_prefix("getInfo ").unwrap().to_string();
            let mut reply = vec![0xFFu8; 4];
            reply.extend_from_slice(
                format!("infoResponse\n\\challenge\\{challenge}\\mapname\\zm_zod\\clients\\1").as_bytes(),
            );
            server.send_to(&reply, from).unwrap();
        });
        let info = query(Ipv4Addr::LOCALHOST.into(), port, Duration::from_secs(2)).expect("answer");
        t.join().unwrap();
        assert_eq!(info.get("mapname").map(String::as_str), Some("zm_zod"));

        // Nothing listening: no answer, no panic.
        let free = next_free_port(40000).unwrap();
        assert!(query(Ipv4Addr::LOCALHOST.into(), free, Duration::from_millis(200)).is_none());

        let held = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let held_port = held.local_addr().unwrap().port();
        assert!(!port_free(held_port));
        assert_ne!(next_free_port(held_port), Some(held_port));
    }

    #[test]
    fn address_kinds() {
        assert_eq!(classify("Wi-Fi", Ipv4Addr::new(192, 168, 1, 20)), Some(AddrKind::Lan));
        assert_eq!(classify("Radmin VPN", Ipv4Addr::new(26, 10, 2, 3)), Some(AddrKind::Radmin));
        assert_eq!(classify("Ethernet 3", Ipv4Addr::new(26, 1, 1, 1)), Some(AddrKind::Radmin));
        assert_eq!(classify("x", Ipv4Addr::new(100, 100, 1, 1)), Some(AddrKind::SharedNat));
        assert_eq!(classify("x", Ipv4Addr::new(8, 8, 8, 8)), Some(AddrKind::Public));
        assert_eq!(classify("lo", Ipv4Addr::LOCALHOST), None);
        assert_eq!(classify("x", Ipv4Addr::new(169, 254, 1, 1)), None);

        let addrs = vec![
            Addr { adapter: "Wi-Fi".into(), ip: Ipv4Addr::new(192, 168, 1, 20), kind: AddrKind::Lan },
            Addr { adapter: "Radmin VPN".into(), ip: Ipv4Addr::new(26, 1, 2, 3), kind: AddrKind::Radmin },
        ];
        assert_eq!(preferred(&addrs).unwrap().kind, AddrKind::Radmin);
        assert_eq!(connect_command("1.2.3.4", 27017, ""), "connect 1.2.3.4:27017");
        assert_eq!(connect_command("1.2.3.4", 27030, "pw"), "password pw; connect 1.2.3.4:27030");
        let help = join_help(27030, "", &addrs);
        assert!(help.contains("connect 26.1.2.3:27030") && help.contains("UDP 27030"));
    }
}
