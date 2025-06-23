use zenoh_config::{
    unwrap_or_default, AutoConnectStrategy, Config, ModeDependent, TargetDependentValue,
};
use zenoh_protocol::core::{WhatAmI, WhatAmIMatcher, ZenohIdProto};

/// Auto-connection manager, combining autoconnect matcher and strategy from the config.
#[derive(Clone, Copy)]
pub(crate) struct AutoConnect {
    zid: ZenohIdProto,
    matcher: WhatAmIMatcher,
    strategy: TargetDependentValue<AutoConnectStrategy>,
}

impl AutoConnect {
    /// Builds an `AutoConnect` from multicast config.
    pub(crate) fn multicast(config: &Config, what: WhatAmI, zid: ZenohIdProto) -> Self {
        Self {
            zid,
            matcher: *unwrap_or_default!(config.scouting().multicast().autoconnect().get(what)),
            strategy: *unwrap_or_default!(config
                .scouting()
                .multicast()
                .autoconnect_strategy()
                .get(what)),
        }
    }

    /// Builds an `AutoConnect` from gossip config.
    pub(crate) fn gossip(config: &Config, what: WhatAmI, zid: ZenohIdProto) -> Self {
        Self {
            zid,
            matcher: *unwrap_or_default!(config.scouting().gossip().autoconnect().get(what)),
            strategy: *unwrap_or_default!(config
                .scouting()
                .gossip()
                .autoconnect_strategy()
                .get(what)),
        }
    }

    /// Builds a disabled `AutoConnect`, with [`is_enabled`](Self::is_enabled) or
    /// [`should_autoconnect`](Self::should_autoconnect) always return false.
    pub(crate) fn disabled() -> Self {
        Self {
            zid: ZenohIdProto::default(),
            matcher: WhatAmIMatcher::empty(),
            strategy: TargetDependentValue::Unique(AutoConnectStrategy::default()),
        }
    }

    /// Returns the autonnection matcher.
    pub(crate) fn matcher(&self) -> WhatAmIMatcher {
        self.matcher
    }

    /// Returns if the autoconnection is enabled for at least one mode.
    pub(crate) fn is_enabled(&self) -> bool {
        !self.matcher.is_empty()
    }

    /// Returns if the node should autoconnect to the other one according to the chosen strategy.
    ///
    /// The goal is to avoid both node to attempt connecting to each other, as it would result into
    /// a waste of resource.
    pub(crate) fn should_autoconnect(&self, to: ZenohIdProto, what: WhatAmI) -> bool {
        let strategy = || match self.strategy.get(what).copied().unwrap_or_default() {
            AutoConnectStrategy::Always => true,
            AutoConnectStrategy::GreaterZid => self.zid > to,
        };
        self.matcher.matches(what) && strategy()
    }
}
