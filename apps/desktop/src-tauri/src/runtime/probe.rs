//! Endpoint parsing and bounded, read-only loopback observation.
use crate::error::DesktopError;
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// How long one connect attempt may take before the address counts as not
/// answering. A full listen backlog drops SYNs, so without a timeout a
/// blocking connect would stall a status read for the OS retry window.
const PROBE_ATTEMPT_TIMEOUT: Duration = Duration::from_millis(300);

/// The loopback target an access address names: which host(s) a probe may
/// touch, at which explicit port. An explicit IP constrains the probe to
/// exactly that address — a same-port listener on the other protocol stack
/// is not evidence about the configured address; only `localhost` licenses
/// trying both stacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopbackTarget {
    Localhost(u16),
    Address(std::net::IpAddr, u16),
}

impl std::fmt::Display for LoopbackTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoopbackTarget::Localhost(port) => write!(f, "localhost:{port}"),
            LoopbackTarget::Address(ip, port) => write!(f, "{ip}:{port}"),
        }
    }
}

/// True when something accepts TCP connections at the target within the
/// attempt timeout, and before `deadline` when one is given. The only
/// "external instance" signal AssetMesh relies on: it refuses a duplicate
/// start on it and reports `external` from read paths; it never signals the
/// process behind it. An expired budget that prevents an attempt is an error,
/// not evidence that the address is silent. Each remaining address receives
/// a share of the remaining time, so IPv4 cannot consume IPv6's whole budget.
pub(crate) fn loopback_target_listening(
    target: &LoopbackTarget,
    deadline: Option<Instant>,
) -> Result<bool, DesktopError> {
    let probe_one = |ip: std::net::IpAddr, port: u16, remaining_addresses: u32| {
        let timeout = match deadline {
            Some(at) => PROBE_ATTEMPT_TIMEOUT
                .min(at.saturating_duration_since(Instant::now()) / remaining_addresses),
            None => PROBE_ATTEMPT_TIMEOUT,
        };
        if timeout.is_zero() {
            return Err(DesktopError::unavailable(
                "service status probe budget was exhausted; retry shortly",
            ));
        }
        Ok(TcpStream::connect_timeout(&std::net::SocketAddr::new(ip, port), timeout).is_ok())
    };
    match target {
        LoopbackTarget::Localhost(port) => {
            let addresses = [
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
            ];
            for (index, ip) in addresses.into_iter().enumerate() {
                if probe_one(ip, *port, (addresses.len() - index) as u32)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        LoopbackTarget::Address(ip, port) => probe_one(*ip, *port, 1),
    }
}

/// Extracts the loopback probe target of an access address, when the address
/// points at this machine and carries an explicit port. Parsed with a real
/// URL parser, so query strings, fragments, and host case cannot defeat it.
/// Remote hosts, non-HTTP schemes, and addresses without a port yield `None`
/// — there is nothing local to check.
pub fn parse_loopback_target(endpoint_url: &str) -> Option<LoopbackTarget> {
    let trimmed = endpoint_url.trim();
    let url = tauri::Url::parse(trimmed).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    // The URL parser removes explicit scheme-default ports (:80/:443).
    // Recover only that explicit marker from the validated original authority;
    // host parsing and validation still belong to the URL parser. A port in a
    // query/fragment must not make an address without a port eligible.
    let port = url.port().or_else(|| {
        let default_port = url.port_or_known_default()?;
        let authority = trimmed
            .split_once("://")?
            .1
            .split(['/', '\\', '?', '#'])
            .next()?;
        let explicit_port = authority.rsplit_once(':')?.1.parse::<u16>().ok()?;
        (explicit_port == default_port).then_some(explicit_port)
    })?;
    let host = url.host_str()?;
    // `localhost` (the parser lowercases domains) may be served on either
    // stack, so the probe may try both. Anything else must be an explicit
    // loopback IP, which pins the probe to exactly that address.
    if host.eq_ignore_ascii_case("localhost") {
        return Some(LoopbackTarget::Localhost(port));
    }
    let ip: std::net::IpAddr = host.trim_matches(['[', ']']).parse().ok()?;
    ip.is_loopback()
        .then_some(LoopbackTarget::Address(ip, port))
}

#[cfg(all(test, unix))]
#[path = "../../tests/support/loopback.rs"]
mod test_loopback;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_explicit_default_ports_without_probing_implicit_ports() {
        for address in ["http://127.0.0.1:80", "HTTP://127.0.0.1:00080?x=1#frag"] {
            assert_eq!(
                parse_loopback_target(address),
                Some(LoopbackTarget::Address(
                    std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                    80
                )),
                "{address}"
            );
        }
        assert_eq!(
            parse_loopback_target("https://LOCALHOST:443?x=1"),
            Some(LoopbackTarget::Localhost(443))
        );
        assert_eq!(
            parse_loopback_target("https://[::1]:443/#frag"),
            Some(LoopbackTarget::Address(
                std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
                443
            ))
        );
        for address in [
            "http://localhost",
            "https://localhost",
            "http://[::1]",
            "https://127.0.0.1?next=http://localhost:443",
        ] {
            assert_eq!(parse_loopback_target(address), None, "{address}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_blocked_connect_uses_only_the_remaining_budget() {
        let (listener, _clients) = test_loopback::saturated_listener();
        let target = LoopbackTarget::Address(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            listener.local_addr().unwrap().port(),
        );
        let began = Instant::now();
        let _ = loopback_target_listening(&target, Some(began + Duration::from_millis(40)));
        assert!(
            began.elapsed() < Duration::from_millis(180),
            "the connect ignored its remaining budget: {:?}",
            began.elapsed()
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_blocked_ipv4_address_leaves_time_for_a_healthy_ipv6_address() {
        let (listener, _clients) = test_loopback::saturated_listener();
        let port = listener.local_addr().unwrap().port();
        let _healthy = std::net::TcpListener::bind((std::net::Ipv6Addr::LOCALHOST, port))
            .expect("healthy IPv6 listener on the same port");
        assert!(loopback_target_listening(
            &LoopbackTarget::Localhost(port),
            Some(Instant::now() + Duration::from_millis(100)),
        )
        .unwrap());
    }

    #[test]
    fn parses_loopback_probe_targets_from_access_addresses() {
        use std::net::{Ipv4Addr, Ipv6Addr};
        // Explicit loopback IPs pin the probe to exactly that address.
        assert_eq!(
            parse_loopback_target("http://127.0.0.1:7861"),
            Some(LoopbackTarget::Address(
                std::net::IpAddr::V4(Ipv4Addr::LOCALHOST),
                7861
            ))
        );
        assert_eq!(
            parse_loopback_target("http://[::1]:7080"),
            Some(LoopbackTarget::Address(
                std::net::IpAddr::V6(Ipv6Addr::LOCALHOST),
                7080
            ))
        );
        // `localhost` licenses trying both protocol stacks.
        assert_eq!(
            parse_loopback_target("http://localhost:8080/"),
            Some(LoopbackTarget::Localhost(8080))
        );
        // Host case, query strings, and fragments cannot defeat the parser.
        assert_eq!(
            parse_loopback_target("https://LOCALHOST:9000/api/v1?x=1#frag"),
            Some(LoopbackTarget::Localhost(9000))
        );
        assert_eq!(
            parse_loopback_target("http://127.0.0.1:7861?x=1"),
            Some(LoopbackTarget::Address(
                std::net::IpAddr::V4(Ipv4Addr::LOCALHOST),
                7861
            ))
        );
        // Nothing local to check.
        assert_eq!(parse_loopback_target("http://192.168.1.5:7861"), None);
        assert_eq!(parse_loopback_target("http://example.com"), None);
        assert_eq!(parse_loopback_target("http://localhost"), None);
        assert_eq!(parse_loopback_target("not a url"), None);
        assert_eq!(parse_loopback_target("ftp://127.0.0.1:21"), None);
    }

    #[test]
    fn a_probe_answers_only_for_the_configured_address() {
        // An IPv6-only listener is invisible to an explicit IPv4 target.
        let v6_listener = std::net::TcpListener::bind("[::1]:0").expect("ipv6 listener");
        let port = v6_listener.local_addr().expect("addr").port();
        assert!(!loopback_target_listening(
            &LoopbackTarget::Address(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), port),
            None
        )
        .unwrap());
        // ...but `localhost` may try both stacks, and an explicit IPv6
        // target addresses it directly.
        assert!(loopback_target_listening(&LoopbackTarget::Localhost(port), None).unwrap());
        assert!(loopback_target_listening(
            &LoopbackTarget::Address(std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST), port),
            None
        )
        .unwrap());

        // The mirror case: an IPv4-only listener is invisible to an explicit
        // IPv6 target.
        let v4_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("ipv4 listener");
        let port = v4_listener.local_addr().expect("addr").port();
        assert!(!loopback_target_listening(
            &LoopbackTarget::Address(std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST), port),
            None
        )
        .unwrap());
    }

    #[test]
    fn a_spent_round_budget_is_unobserved_instead_of_not_answering() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let port = listener.local_addr().expect("addr").port();
        let target =
            LoopbackTarget::Address(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), port);
        assert!(loopback_target_listening(&target, None).unwrap());
        let spent = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("monotonic clock has been up for a second");
        let error = loopback_target_listening(&target, Some(spent)).unwrap_err();
        assert_eq!(error.category, "unavailable");
    }
}
