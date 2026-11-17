//! UPnP IGD: find the router, and ask it to open the torrent port.
//!
//! The discovery is the one UPnP specifies: an `M-SEARCH` on the SSDP
//! multicast group, answered with the address of the device description.
//! The description names the control address of the WAN connection service;
//! the mapping is a SOAP `AddPortMapping` sent there, and its removal a
//! `DeletePortMapping`. A network with no IGD is a normal result — nothing
//! to map through — not an error.

use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::UdpSocket;

use crate::error::TorrentError;

/// The SSDP group and port, as UPnP specifies them.
pub const SSDP: SocketAddr = SocketAddr::new(
    std::net::IpAddr::V4(std::net::Ipv4Addr::new(239, 255, 255, 250)),
    1900,
);

/// The search that asks for an internet gateway device.
pub fn search_message() -> String {
    "M-SEARCH * HTTP/1.1\r\n\
     HOST: 239.255.255.250:1900\r\n\
     MAN: \"ssdp:discover\"\r\n\
     MX: 2\r\n\
     ST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\
     \r\n"
        .to_string()
}

/// The description address out of a search answer.
pub fn location_from_response(text: &str) -> Option<String> {
    for line in text.split("\r\n") {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("location") {
            let value = value.trim();
            if value.starts_with("http://") || value.starts_with("https://") {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Ask the network for an IGD. `target` is the SSDP group on a real network;
/// a test points it at a closed port to hear the silence of no gateway.
pub async fn discover(target: SocketAddr, wait: Duration) -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").await.ok()?;
    socket
        .send_to(search_message().as_bytes(), target)
        .await
        .ok()?;
    let mut buffer = vec![0u8; 4096];
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            return None;
        }
        let received = tokio::time::timeout(left, socket.recv_from(&mut buffer)).await;
        let Ok(Ok((length, _))) = received else {
            return None;
        };
        let text = String::from_utf8_lossy(&buffer[..length]);
        if let Some(location) = location_from_response(&text) {
            return Some(location);
        }
    }
}

/// The control address of the WAN connection service, out of the device
/// description, resolved against the address the description came from.
pub fn control_url_from_description(xml: &str, base: &str) -> Option<(String, String)> {
    for service in xml.split("<service>").skip(1) {
        let Some(end) = service.find("</service>") else {
            continue;
        };
        let service = &service[..end];
        let kind = tag_text(service, "serviceType")?;
        if !kind.contains("WANIPConnection:1") && !kind.contains("WANPPPConnection:1") {
            continue;
        }
        let control = tag_text(service, "controlURL")?;
        return Some((kind, resolve_url(base, &control)));
    }
    None
}

/// The text of one tag, when it is there.
fn tag_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].trim().to_string())
}

/// A control address against the address the description came from: an
/// absolute address stands, a path keeps the host, a name keeps the folder.
pub fn resolve_url(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    let Some((scheme, rest)) = base.split_once("://") else {
        return path.to_string();
    };
    let (host, folder) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, "/"),
    };
    if path.starts_with('/') {
        return format!("{scheme}://{host}{path}");
    }
    let folder = &folder[..folder.rfind('/').map(|at| at + 1).unwrap_or(folder.len())];
    format!("{scheme}://{host}{folder}{path}")
}

/// The SOAP body that asks for a mapping.
pub fn add_port_mapping_body(
    service: &str,
    port: u16,
    internal: std::net::Ipv4Addr,
    lease: u32,
) -> String {
    format!(
        "<?xml version=\"1.0\"?>\
         <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
         <s:Body><u:AddPortMapping xmlns:u=\"{service}\">\
         <NewRemoteHost></NewRemoteHost>\
         <NewExternalPort>{port}</NewExternalPort>\
         <NewProtocol>TCP</NewProtocol>\
         <NewInternalPort>{port}</NewInternalPort>\
         <NewInternalClient>{internal}</NewInternalClient>\
         <NewEnabled>1</NewEnabled>\
         <NewPortMappingDescription>B Download Manager</NewPortMappingDescription>\
         <NewLeaseDuration>{lease}</NewLeaseDuration>\
         </u:AddPortMapping></s:Body></s:Envelope>"
    )
}

/// The SOAP body that gives the mapping back.
pub fn delete_port_mapping_body(service: &str, port: u16) -> String {
    format!(
        "<?xml version=\"1.0\"?>\
         <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
         <s:Body><u:DeletePortMapping xmlns:u=\"{service}\">\
         <NewRemoteHost></NewRemoteHost>\
         <NewExternalPort>{port}</NewExternalPort>\
         <NewProtocol>TCP</NewProtocol>\
         </u:DeletePortMapping></s:Body></s:Envelope>"
    )
}

/// The header the SOAP action travels in.
pub fn soap_action(service: &str, method: &str) -> String {
    format!("\"{service}#{method}\"")
}

/// Whether the router took the request, or the words it refused with.
pub fn answer_result(status: u16, body: &str) -> Result<(), TorrentError> {
    if status == 200 {
        return Ok(());
    }
    let reason = tag_text(body, "faultstring")
        .or_else(|| tag_text(body, "errorDescription"))
        .unwrap_or_else(|| format!("the router answered {status}"));
    Err(TorrentError::Network(format!(
        "the router refused the port mapping: {reason}"
    )))
}

/// A mapping the router holds.
#[derive(Clone, Debug)]
pub struct Mapping {
    /// The service the request goes to.
    pub service: String,
    /// The control address it goes to.
    pub control: String,
    /// The port that was opened.
    pub port: u16,
    /// The seconds the router keeps it.
    pub lease: u32,
}

/// Ask the router for a mapping: discover, read the description, send the
/// SOAP request. `Ok(None)` is a network with no IGD — a normal answer.
pub async fn map(
    port: u16,
    internal: std::net::Ipv4Addr,
    lease: u32,
) -> Result<Option<Mapping>, TorrentError> {
    let Some(location) = discover(SSDP, Duration::from_secs(3)).await else {
        return Ok(None);
    };
    // The description is fetched directly: it is on the local network, and
    // a proxy off it would never reach the router.
    let client = crate::tracker::http_client(None)?;
    let description = client
        .get(&location)
        .send()
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?
        .text()
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;
    let Some((service, control)) = control_url_from_description(&description, &location) else {
        return Ok(None);
    };
    let body = add_port_mapping_body(&service, port, internal, lease);
    let response = client
        .post(&control)
        .header("SOAPACTION", soap_action(&service, "AddPortMapping"))
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .body(body)
        .send()
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    answer_result(status, &text)?;
    Ok(Some(Mapping {
        service,
        control,
        port,
        lease,
    }))
}

/// Ask the router for the mapping again, before its lease runs out.
pub async fn refresh(mapping: &Mapping, internal: std::net::Ipv4Addr) -> Result<(), TorrentError> {
    let client = crate::tracker::http_client(None)?;
    let body = add_port_mapping_body(&mapping.service, mapping.port, internal, mapping.lease);
    let response = client
        .post(&mapping.control)
        .header(
            "SOAPACTION",
            soap_action(&mapping.service, "AddPortMapping"),
        )
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .body(body)
        .send()
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    answer_result(status, &text)
}

/// Give the mapping back.
pub async fn unmap(mapping: &Mapping) -> Result<(), TorrentError> {
    let client = crate::tracker::http_client(None)?;
    let response = client
        .post(&mapping.control)
        .header(
            "SOAPACTION",
            soap_action(&mapping.service, "DeletePortMapping"),
        )
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .body(delete_port_mapping_body(&mapping.service, mapping.port))
        .send()
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    answer_result(status, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESCRIPTION: &str = r#"<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
 <device>
  <deviceType>urn:schemas-upnp-org:device:InternetGatewayDevice:1</deviceType>
  <deviceList><device>
   <deviceType>urn:schemas-upnp-org:device:WANDevice:1</deviceType>
   <deviceList><device>
    <deviceType>urn:schemas-upnp-org:device:WANConnectionDevice:1</deviceType>
    <serviceList>
     <service>
      <serviceType>urn:schemas-upnp-org:service:WANCommonInterfaceConfig:1</serviceType>
      <controlURL>/common</controlURL>
     </service>
     <service>
      <serviceType>urn:schemas-upnp-org:service:WANIPConnection:1</serviceType>
      <controlURL>/ipc</controlURL>
     </service>
    </serviceList>
   </device></deviceList>
  </device></deviceList>
 </device>
</root>"#;

    #[test]
    fn the_search_asks_for_a_gateway() {
        let text = search_message();
        assert!(text.starts_with("M-SEARCH * HTTP/1.1\r\n"));
        assert!(text.contains("MAN: \"ssdp:discover\""));
        assert!(text.contains("ST: urn:schemas-upnp-org:device:InternetGatewayDevice:1"));
    }

    #[test]
    fn the_location_is_read_from_an_answer() {
        let answer = "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\nLOCATION: http://192.168.1.1:49152/desc.xml\r\nST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\r\n";
        assert_eq!(
            location_from_response(answer).as_deref(),
            Some("http://192.168.1.1:49152/desc.xml")
        );
        assert_eq!(location_from_response("HTTP/1.1 200 OK\r\n\r\n"), None);
    }

    #[test]
    fn the_wan_service_is_the_one_taken() {
        let (service, control) =
            control_url_from_description(DESCRIPTION, "http://192.168.1.1:49152/desc.xml")
                .expect("the service");
        assert!(service.contains("WANIPConnection:1"));
        assert_eq!(control, "http://192.168.1.1:49152/ipc");
        assert!(control_url_from_description("<root></root>", "http://x/").is_none());
    }

    #[test]
    fn a_control_address_resolves_against_its_base() {
        let base = "http://192.168.1.1:49152/desc.xml";
        assert_eq!(resolve_url(base, "/ipc"), "http://192.168.1.1:49152/ipc");
        assert_eq!(resolve_url(base, "ipc"), "http://192.168.1.1:49152/ipc");
        assert_eq!(
            resolve_url(base, "http://other:1/ctrl"),
            "http://other:1/ctrl"
        );
        assert_eq!(
            resolve_url("http://192.168.1.1/desc.xml", "sub/ipc"),
            "http://192.168.1.1/sub/ipc"
        );
    }

    #[test]
    fn the_soap_bodies_carry_the_mapping() {
        let service = "urn:schemas-upnp-org:service:WANIPConnection:1";
        let body = add_port_mapping_body(service, 6881, "192.168.1.10".parse().unwrap(), 3600);
        assert!(body.contains("<u:AddPortMapping"));
        assert!(body.contains("<NewExternalPort>6881</NewExternalPort>"));
        assert!(body.contains("<NewProtocol>TCP</NewProtocol>"));
        assert!(body.contains("<NewInternalClient>192.168.1.10</NewInternalClient>"));
        assert!(body.contains("<NewLeaseDuration>3600</NewLeaseDuration>"));
        let body = delete_port_mapping_body(service, 6881);
        assert!(body.contains("<u:DeletePortMapping"));
        assert!(body.contains("<NewExternalPort>6881</NewExternalPort>"));
        assert_eq!(
            soap_action(service, "AddPortMapping"),
            "\"urn:schemas-upnp-org:service:WANIPConnection:1#AddPortMapping\""
        );
    }

    #[test]
    fn a_refusal_carries_the_routers_words() {
        assert!(answer_result(200, "").is_ok());
        let fault = "<s:Envelope><s:Body><s:Fault><faultstring>ConflictInMappingEntry</faultstring></s:Fault></s:Body></s:Envelope>";
        let error = answer_result(500, fault).unwrap_err();
        assert!(error.to_string().contains("ConflictInMappingEntry"));
        assert!(answer_result(404, "").is_err());
    }

    #[tokio::test]
    async fn a_network_with_no_igd_is_a_clean_nothing() {
        // A closed port on the loopback: no gateway answers, and the
        // discovery comes back with nothing instead of an error.
        let closed: SocketAddr = "127.0.0.1:1".parse().unwrap();
        assert!(discover(closed, Duration::from_millis(300)).await.is_none());
    }
}
