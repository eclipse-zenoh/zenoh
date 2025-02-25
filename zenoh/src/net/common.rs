use zenoh_config::{
    unwrap_or_default, AutoConnectStrategy, Config, ModeDependent, TargetDependentValue,
};
use zenoh_link::Locator;
use zenoh_protocol::core::{Metadata, WhatAmI, WhatAmIMatcher, ZenohIdProto};

/// Auto-connection manager, combining autoconnect matcher and strategy from the config.
#[derive(Clone)]
pub(crate) struct AutoConnect {
    zid: ZenohIdProto,
    matcher: WhatAmIMatcher,
    groups: Option<Vec<String>>,
    strategy: TargetDependentValue<AutoConnectStrategy>,
}

impl AutoConnect {
    /// Builds an `AutoConnect` from multicast config.
    pub(crate) fn multicast(config: &Config, what: WhatAmI) -> Self {
        Self {
            zid: (*config.id()).into(),
            matcher: *unwrap_or_default!(config.scouting().multicast().autoconnect().get(what)),
            groups: config.groups().connectivity().clone(),
            strategy: *unwrap_or_default!(config
                .scouting()
                .multicast()
                .autoconnect_strategy()
                .get(what)),
        }
    }

    /// Builds an `AutoConnect` from gossip config.
    pub(crate) fn gossip(config: &Config, what: WhatAmI) -> Self {
        Self {
            zid: (*config.id()).into(),
            matcher: *unwrap_or_default!(config.scouting().multicast().autoconnect().get(what)),
            groups: config.groups().connectivity().clone(),
            strategy: *unwrap_or_default!(config
                .scouting()
                .multicast()
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
            groups: None,
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
    pub(crate) fn should_autoconnect(
        &self,
        to: ZenohIdProto,
        what: WhatAmI,
        locators: &[Locator],
    ) -> Vec<Locator> {
        let mut locs = vec![];

        // Check wether we are matching the right entity: peer/router
        if !self.matcher.matches(what) {
            return locs;
        }

        // Check wether we are allowed to connect to it because of the autoconnect strategy
        match self.strategy.get(what).copied().unwrap_or_default() {
            AutoConnectStrategy::Always => {}
            AutoConnectStrategy::GreaterZid => {
                if self.zid <= to {
                    return locs;
                }
            }
        };

        // Check wether we are allowed to connect to it because of the groups matchin
        if let Some(gs) = self.groups.as_ref() {
            // Filter out locators not matching any of the groups
            for l in locators.iter() {
                for gl in l.metadata().values(Metadata::GROUPS) {
                    if gs.iter().any(|g| g == gl) {
                        locs.push(l.clone());
                    } else {
                        tracing::debug!("Will not connect to {} ({:?}) via {} because it does not participate in any of these groups: {:?}", to, what, l, gs);
                    }
                }
            }
        } else {
            locs = locators.to_vec();
        };

        locs
    }
}
