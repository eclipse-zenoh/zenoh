use zenoh_config::{
    unwrap_or_default, AutoConnectStrategy, Config, ModeDependent, ModeDependentValue,
};
use zenoh_protocol::core::{WhatAmI, WhatAmIMatcher, ZenohIdProto};

#[derive(Clone, Copy)]
pub(crate) struct AutoConnect {
    zid: ZenohIdProto,
    matcher: WhatAmIMatcher,
    strategy: ModeDependentValue<AutoConnectStrategy>,
}

impl AutoConnect {
    pub(crate) fn multicast(config: &Config, what: WhatAmI) -> Self {
        Self {
            zid: (*config.id()).into(),
            matcher: *unwrap_or_default!(config.scouting().multicast().autoconnect().get(what)),
            strategy: unwrap_or_default!(config.scouting().multicast().autoconnect_strategy()),
        }
    }

    pub(crate) fn gossip(config: &Config, what: WhatAmI) -> Self {
        Self {
            zid: (*config.id()).into(),
            matcher: *unwrap_or_default!(config.scouting().multicast().autoconnect().get(what)),
            strategy: unwrap_or_default!(config.scouting().multicast().autoconnect_strategy()),
        }
    }

    pub(crate) fn disabled() -> Self {
        Self {
            zid: ZenohIdProto::default(),
            matcher: WhatAmIMatcher::empty(),
            strategy: ModeDependentValue::Unique(AutoConnectStrategy::default()),
        }
    }

    // pub(crate) fn new(
    //     zid: impl Into<ZenohIdProto>,
    //     matcher: WhatAmIMatcher,
    //     strategy: ModeDependentValue<AutoConnectStrategy>,
    // ) -> Self {
    //     Self {
    //         zid,
    //         matcher,
    //         strategy,
    //     }
    // }

    pub(crate) fn matcher(&self) -> WhatAmIMatcher {
        self.matcher
    }

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
