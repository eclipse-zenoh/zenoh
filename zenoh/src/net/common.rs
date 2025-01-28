use zenoh_config::AutoConnectStrategy;
use zenoh_protocol::core::ZenohIdProto;

/// Returns if the node should autoconnect to the other one according to the give strategy.
///
/// The goal is to avoid both node to attempt connecting to each other, as it would result into
/// a waste of resource.
pub(crate) fn should_autoconnect(
    strategy: Option<AutoConnectStrategy>,
    self_zid: ZenohIdProto,
    other_zid: ZenohIdProto,
) -> bool {
    match strategy {
        Some(AutoConnectStrategy::GreaterZid) => self_zid > other_zid,
        None => true,
    }
}
