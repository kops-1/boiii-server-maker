//! "Can my friends reach me?" - a read-only look at this PC's network side:
//! addresses, the public address, the Windows firewall, and whether the
//! BOIII server list knows the server. Nothing is changed.

use crate::net::{self, Addr, AddrKind};
use crate::server::Env;
use std::net::{Ipv4Addr, ToSocketAddrs, UdpSocket};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::time::{Duration, Instant};

pub const MASTER_HOSTS: [&str; 2] = ["master.ezz.lol", "m.ezz.lol"];
pub const MASTER_PORT: u16 = 20810;
/// Answers with the caller's public IPv4 address as plain text.
pub const PUBLIC_IP_URL: &str = "https://api.ipify.org";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Checks and tidies a tunnel address such as `name.joinmc.link:12345` or
/// `147.185.221.20:31337`. None if it is not `host:port`.
pub fn normalize_tunnel(address: &str) -> Option<String> {
    let value = address.trim();
    if value.is_empty() || value.len() > 289 || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let (host, port) = value.rsplit_once(':')?;
    if host.contains(':') || host.is_empty() {
        return None; // URL schemes and IPv6 are not accepted
    }
    let port: u16 = port.parse().ok().filter(|p| *p > 0)?;
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Some(format!("{ip}:{port}"));
    }
    let host = host.trim_end_matches('.').to_lowercase();
    let label_ok = |l: &str| {
        !l.is_empty()
            && l.len() <= 63
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    if host.len() > 253
        || host.chars().all(|c| c.is_ascii_digit() || c == '.')
        || !host.split('.').all(label_ok)
    {
        return None;
    }
    Some(format!("{host}:{port}"))
}

/// This PC's public IPv4 address as an outside service sees it.
pub fn public_ip(work_dir: &Path) -> Option<Ipv4Addr> {
    let file = work_dir.join("public-ip.txt");
    crate::download::download(PUBLIC_IP_URL, &file).ok()?;
    let text = std::fs::read_to_string(&file).ok()?;
    let _ = std::fs::remove_file(&file);
    text.trim().parse().ok()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MasterResult {
    pub host: String,
    pub responded: bool,
    /// The server (public address and port) is in the list.
    pub listed: bool,
    /// Servers in the answer.
    pub entries: u32,
    pub error: String,
}

/// Reads one `getserversResponse` packet: entries are 4 address bytes, a
/// 2-byte port (high byte first) and a `\` separator. Returns (entries, listed).
pub fn parse_master_packet(data: &[u8], target: Ipv4Addr, port: u16) -> Option<(u32, bool)> {
    let marker = b"getserversResponse";
    let start = data.windows(marker.len()).position(|w| w == marker)?;
    let sep = data[start + marker.len()..].iter().position(|b| *b == b'\\')? + start + marker.len();
    let mut entries = 0;
    let mut listed = false;
    let mut i = sep + 1;
    while i + 6 < data.len() && data[i + 6] == b'\\' {
        entries += 1;
        let p = u16::from(data[i + 4]) * 256 + u16::from(data[i + 5]);
        if data[i..i + 4] == target.octets() && p == port {
            listed = true;
        }
        i += 7;
    }
    Some((entries, listed))
}

/// Asks one BOIII master server for its list and looks for this server in it.
pub fn query_master(host: &str, target: Ipv4Addr, port: u16, protocol: u32) -> MasterResult {
    let mut result = MasterResult { host: host.to_string(), ..Default::default() };
    let addr = match (host, MASTER_PORT).to_socket_addrs().ok().and_then(|mut a| a.find(|a| a.is_ipv4())) {
        Some(a) => a,
        None => {
            result.error = "name not found".into();
            return result;
        }
    };
    let Ok(sock) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) else {
        result.error = "no socket".into();
        return result;
    };
    let _ = sock.set_read_timeout(Some(Duration::from_millis(2500)));
    let mut request = vec![0xFFu8; 4];
    request.extend_from_slice(format!("getservers T7 {protocol} full empty").as_bytes());
    if sock.send_to(&request, addr).is_err() {
        result.error = "could not send".into();
        return result;
    }
    // A long list arrives in several packets.
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut buf = vec![0u8; 65536];
    while Instant::now() < deadline {
        let Ok((n, _)) = sock.recv_from(&mut buf) else { break };
        match parse_master_packet(&buf[..n], target, port) {
            Some((entries, listed)) => {
                result.responded = true;
                result.entries += entries;
                result.listed |= listed;
                let _ = sock.set_read_timeout(Some(Duration::from_millis(400)));
            }
            None => break,
        }
    }
    if !result.responded {
        result.error = "no answer".into();
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Firewall {
    /// Number of enabled inbound allow rules for the server program.
    Allowed(u32),
    /// No allow rule for the program (Windows asks on first start, or blocks silently).
    NoRule,
    /// An enabled block rule for the program exists.
    Blocked,
    Unknown(String),
}

/// Reads the firewall rules that name the server program. Read-only.
pub fn firewall_for(exe: &Path) -> Firewall {
    let Some(ps) = crate::paths::system32()
        .map(|d| d.join("WindowsPowerShell").join("v1.0").join("powershell.exe"))
        .filter(|p| p.is_file())
    else {
        return Firewall::Unknown("PowerShell not found".into());
    };
    let program = exe.display().to_string().replace('\'', "''");
    let script = format!(
        "$a=0;$b=0;Get-NetFirewallApplicationFilter -PolicyStore ActiveStore -Program '{program}' -ErrorAction SilentlyContinue | ForEach-Object {{ $r=$_ | Get-NetFirewallRule; if($r.Enabled -eq 'True' -and $r.Direction -eq 'Inbound'){{ if($r.Action -eq 'Allow'){{$a++}}else{{$b++}} }} }};Write-Output ('allow='+$a+' block='+$b)"
    );
    let out = std::process::Command::new(ps)
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(script)
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match out {
        Ok(o) => parse_firewall(&String::from_utf8_lossy(&o.stdout)),
        Err(e) => Firewall::Unknown(e.to_string()),
    }
}

pub fn parse_firewall(output: &str) -> Firewall {
    let grab = |key: &str| -> Option<u32> {
        let rest = output.split(key).nth(1)?;
        rest.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()
    };
    match (grab("allow="), grab("block=")) {
        (_, Some(b)) if b > 0 => Firewall::Blocked,
        (Some(a), _) if a > 0 => Firewall::Allowed(a),
        (Some(_), _) => Firewall::NoRule,
        _ => Firewall::Unknown("no answer from the firewall query".into()),
    }
}

#[derive(Debug, Clone)]
pub struct Report {
    pub server_port: u16,
    pub server_answers: bool,
    pub addresses: Vec<Addr>,
    pub public_ip: Option<Ipv4Addr>,
    pub firewall: Firewall,
    pub masters: Vec<MasterResult>,
    pub tunnel: Option<String>,
}

/// Runs all checks. Takes several seconds (firewall query, master servers).
pub fn check(env: &Env, log: &dyn Fn(&str)) -> Report {
    let status = crate::server::status(env);
    let port = status.port;
    log("Looking at this PC's addresses...");
    let addresses = net::addresses();
    log("Asking for the public address...");
    let public_ip = public_ip(&env.data_dir.join("tmp"));
    log("Reading the Windows firewall rules for the server...");
    let firewall = firewall_for(&env.client_exe());
    let mut masters = Vec::new();
    if let Some(ip) = public_ip {
        log("Asking the BOIII server list...");
        let protocol = status.info.get("protocol").and_then(|p| p.parse().ok()).unwrap_or(8);
        for host in MASTER_HOSTS {
            masters.push(query_master(host, ip, port, protocol));
        }
    }
    Report {
        server_port: port,
        server_answers: status.state == crate::server::State::Running,
        addresses,
        public_ip,
        firewall,
        masters,
        tunnel: env.settings.tunnel_address().map(|s| s.to_string()),
    }
}

/// One finding: good, a warning, or just information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Good,
    Warn,
    Info,
}

/// The report as short findings in plain language, most useful first.
pub fn findings(r: &Report) -> Vec<(Level, String)> {
    let mut out = Vec::new();
    out.push(if r.server_answers {
        (Level::Good, format!("The server is running and answers on this PC (UDP port {}).", r.server_port))
    } else {
        (Level::Warn, "The server is not running, so only part of this check means something. Start it and check again.".into())
    });

    match &r.firewall {
        Firewall::Allowed(_) => out.push((Level::Good, "Windows Firewall has an allow rule for the server program.".into())),
        Firewall::NoRule => out.push((
            Level::Warn,
            format!("Windows Firewall has no allow rule for the server program. Start the server and choose Allow when Windows asks, or add an inbound rule for UDP port {} yourself.", r.server_port),
        )),
        Firewall::Blocked => out.push((Level::Warn, "Windows Firewall has a rule that BLOCKS the server program. Remove or change it in Windows Security > Firewall > Advanced settings > Inbound Rules.".into())),
        Firewall::Unknown(why) => out.push((Level::Info, format!("The firewall could not be read ({why})."))),
    }

    if let Some(t) = &r.tunnel {
        out.push((
            Level::Good,
            format!("Invites use your tunnel address {t}. Keep the tunnel program running while you host."),
        ));
    }
    if let Some(a) = r.addresses.iter().find(|a| a.kind == AddrKind::Radmin) {
        out.push((
            Level::Good,
            format!(
                "Radmin VPN is active ({}). Friends in your Radmin network can join with that address.",
                a.ip
            ),
        ));
    }

    match r.public_ip {
        Some(ip) => {
            let o = ip.octets();
            let shared = (o[0] == 100 && (64..128).contains(&o[1])) || ip.is_private();
            out.push((Level::Info, format!("Your public address is {ip}.")));
            if shared {
                out.push((Level::Warn, "That is a shared provider address (carrier-grade NAT). Port forwarding cannot work; use Radmin VPN or a tunnel.".into()));
            } else {
                out.push((
                    Level::Info,
                    format!(
                        "For friends over the internet without a VPN: forward UDP port {} in your router to this PC. Compare the router's WAN/internet address with {ip}: if they differ, the provider shares the address (common on 4G/5G) and forwarding cannot work.",
                        r.server_port
                    ),
                ));
            }
        }
        None => out.push((
            Level::Info,
            "The public address could not be found (no internet, or the lookup service is unreachable)."
                .into(),
        )),
    }

    if !r.masters.is_empty() {
        let responded: Vec<&MasterResult> = r.masters.iter().filter(|m| m.responded).collect();
        if responded.is_empty() {
            out.push((Level::Info, "The BOIII server list did not answer.".into()));
        } else if responded.iter().any(|m| m.listed) {
            out.push((Level::Good, "The BOIII server list shows your server. This does not prove friends can reach it, only that the server announced itself.".into()));
        } else if r.server_answers {
            out.push((Level::Info, "The BOIII server list does not show your server (normal for a few minutes after a start, and behind a shared address).".into()));
        }
    }
    out
}

pub fn format(r: &Report) -> String {
    findings(r)
        .iter()
        .map(|(level, text)| {
            let mark = match level {
                Level::Good => "[ OK ]",
                Level::Warn => "[ !! ]",
                Level::Info => "[info]",
            };
            format!("{mark} {text}")
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunnel_addresses() {
        assert_eq!(normalize_tunnel(" Name.JoinMC.link:12345 ").as_deref(), Some("name.joinmc.link:12345"));
        assert_eq!(normalize_tunnel("147.185.221.20:31337").as_deref(), Some("147.185.221.20:31337"));
        assert_eq!(normalize_tunnel("host.example.:27017").as_deref(), Some("host.example:27017"));
        for bad in [
            "",
            "host",
            "host:",
            ":123",
            "host:0",
            "host:70000",
            "http://host:123",
            "[::1]:123",
            "a b:123",
            "-bad.com:1",
            "1.2.3:55",
            "host:12 3",
            "ho\"st:1",
        ] {
            assert_eq!(normalize_tunnel(bad), None, "{bad}");
        }
    }

    #[test]
    fn master_packets() {
        let target = Ipv4Addr::new(51, 235, 8, 110);
        let mut packet = vec![0xFF; 4];
        packet.extend_from_slice(b"getserversResponse\n\0\\");
        packet.extend_from_slice(&[1, 2, 3, 4, 0x69, 0x89, b'\\']); // 1.2.3.4:27017
        packet.extend_from_slice(&[51, 235, 8, 110, 0x69, 0x8A, b'\\']); // target, port 27018
        packet.extend_from_slice(&[51, 235, 8, 110, 0x69, 0x89, b'\\']); // target, port 27017
        packet.extend_from_slice(b"EOT\0\0\0");
        assert_eq!(parse_master_packet(&packet, target, 27017), Some((3, true)));
        assert_eq!(parse_master_packet(&packet, target, 27030), Some((3, false)));
        assert_eq!(parse_master_packet(&packet, Ipv4Addr::new(9, 9, 9, 9), 27017), Some((3, false)));
        assert_eq!(parse_master_packet(b"\xff\xff\xff\xffsomethingElse", target, 27017), None);
        assert_eq!(parse_master_packet(b"getserversResponse", target, 27017), None);
    }

    #[test]
    fn firewall_answers() {
        assert_eq!(parse_firewall("allow=2 block=0\r\n"), Firewall::Allowed(2));
        assert_eq!(parse_firewall("allow=0 block=0"), Firewall::NoRule);
        assert_eq!(parse_firewall("allow=3 block=1"), Firewall::Blocked);
        assert!(matches!(parse_firewall("something went wrong"), Firewall::Unknown(_)));
    }

    #[test]
    fn findings_explain_the_common_cases() {
        let base = Report {
            server_port: 27017,
            server_answers: true,
            addresses: vec![Addr {
                adapter: "Radmin VPN".into(),
                ip: Ipv4Addr::new(26, 1, 2, 3),
                kind: AddrKind::Radmin,
            }],
            public_ip: Some(Ipv4Addr::new(100, 70, 1, 2)),
            firewall: Firewall::NoRule,
            masters: vec![MasterResult {
                host: "m".into(),
                responded: true,
                listed: false,
                entries: 100,
                error: String::new(),
            }],
            tunnel: None,
        };
        let text = format(&base);
        assert!(text.contains("[ OK ] The server is running"));
        assert!(text.contains("no allow rule") && text.contains("UDP port 27017"));
        assert!(text.contains("Radmin VPN is active (26.1.2.3)"));
        assert!(text.contains("carrier-grade NAT"));
        assert!(text.contains("does not show your server"));

        let mut good = base.clone();
        good.public_ip = Some(Ipv4Addr::new(51, 235, 8, 110));
        good.firewall = Firewall::Allowed(1);
        good.masters[0].listed = true;
        good.tunnel = Some("x.joinmc.link:1".into());
        let text = format(&good);
        assert!(
            text.contains("forward UDP port 27017")
                && text.contains("shows your server")
                && text.contains("tunnel address x.joinmc.link:1")
        );
        assert!(!text.contains("carrier-grade"));

        let mut off = base;
        off.server_answers = false;
        off.public_ip = None;
        off.masters.clear();
        let text = format(&off);
        assert!(text.contains("not running") && text.contains("could not be found"));
    }
}
