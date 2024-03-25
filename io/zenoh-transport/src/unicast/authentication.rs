use zenoh_link::{LinkAuthId, LinkAuthType};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthId {
    CertCommonName(String),
    Username(Vec<u8>),
    None,
}
impl From<LinkAuthId> for AuthId {
    fn from(lid: LinkAuthId) -> Self {
        match (lid.get_type(), lid.get_value()) {
            (LinkAuthType::Tls | LinkAuthType::Quic, Some(auth_value)) => {
                AuthId::CertCommonName(auth_value.clone())
            }
            _ => AuthId::None,
        }
    }
}
