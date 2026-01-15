//
// Copyright (c) 2026 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use zenoh_config::{
    gateway::{GatewayFiltersConf, GatewayPresetConf, GatewaySouthConf},
    ExpandedConfig, Interface, WhatAmI,
};
#[allow(unused_imports)]
use zenoh_core::polyfill::*;
use zenoh_protocol::core::{Bound, Region};
use zenoh_result::ZResult;
use zenoh_transport::TransportPeer;

/// Computes the _transient_ [`Bound`] of a remote.
///
/// This method is used during the Open phase of establishment to decide whether a remote is
/// south-bound using the [`zenoh_protocol::transport::open::ext::RemoteBound`] extension.
#[tracing::instrument(level = "debug", skip(peer, config), fields(peer.zid = %peer.zid.short(), peer.mode = %peer.whatami), ret)]
pub(crate) fn compute_transient_bound_of(
    peer: &TransportPeer,
    config: &ExpandedConfig,
) -> ZResult<Option<Bound>> {
    Ok(compute_transient_region_of(peer, config)?.map(|r| r.bound()))
}

/// Computes the [`Region`] and _remote_ [`Bound`] of a remote.
#[tracing::instrument(level = "debug", skip(peer, config), fields(peer.zid = %peer.zid.short(), peer.mode = %peer.whatami), ret)]
pub(crate) fn compute_region_of(
    peer: &TransportPeer,
    config: &ExpandedConfig,
    transient_remote_bound: Option<&Bound>,
) -> ZResult<(Region, Bound)> {
    let mode = config.mode();
    let remote_mode = peer.whatami;
    let transient_region = compute_transient_region_of(peer, config)?;

    match (transient_region.map(|r| r.bound()), transient_remote_bound) {
        (None, None) => compute_auto_region(mode, remote_mode),
        (None, Some(Bound::South)) => Ok((Region::North, Bound::South)),
        (Some(Bound::South), None) => Ok((transient_region.unwrap(), Bound::North)),
        (None, Some(Bound::North)) => {
            let (auto_region, auto_remote_bound) = compute_auto_region(mode, remote_mode)?;

            // NOTE(regions): in this case, I am not configured to be a gateway, nor does the auto
            // mode allow me to put the remote in the north region. We _could_ in the future adjust
            // this to put the remote in a subregion.
            if !auto_remote_bound.is_north() {
                bail!("Remote's custom configuration conflicts with auto preset")
            } else {
                Ok((auto_region, Bound::North))
            }
        }
        (Some(Bound::North), None) => {
            let (auto_region, auto_remote_bound) = compute_auto_region(mode, remote_mode)?;

            // NOTE(regions): see above comment for the symmetric case.
            if auto_region.bound().is_south() {
                bail!("Remote's auto preset conflicts with custom configuration")
            } else {
                Ok((Region::North, auto_remote_bound))
            }
        }
        (Some(Bound::North), Some(Bound::North)) => {
            if mode != remote_mode {
                bail!("North-north {mode}-{remote_mode} configuration (invalid)")
            } else {
                Ok((Region::North, Bound::North))
            }
        }
        (Some(Bound::South), Some(Bound::South)) => {
            bail!("South-south configuration (invalid)")
        }
        (Some(Bound::North), Some(Bound::South)) | (Some(Bound::South), Some(Bound::North)) => {
            Ok((transient_region.unwrap(), *transient_remote_bound.unwrap()))
        }
    }
}

fn compute_transient_region_of(
    peer: &TransportPeer,
    config: &ExpandedConfig,
) -> ZResult<Option<Region>> {
    match config.gateway.south.clone().unwrap_or_default() {
        GatewaySouthConf::Preset(GatewayPresetConf::Auto) => Ok(None),
        GatewaySouthConf::Custom(subregions) => {
            if let Some(id) = subregions
                .iter()
                .position(|s| is_match(s.filters.as_deref(), peer))
            {
                if peer.whatami.is_router() && !config.mode().is_router() {
                    bail!("Router regions cannot be subregions of non-router regions (unsupported)")
                }

                Ok(Some(Region::South {
                    id,
                    mode: peer.whatami,
                }))
            } else {
                Ok(Some(Region::North))
            }
        }
    }
}

/// Computes the _auto_ [`Region`] and _remote_ [`Bound`] of a remote.
fn compute_auto_region(mode: WhatAmI, remote_mode: WhatAmI) -> ZResult<(Region, Bound)> {
    match (mode, remote_mode) {
        (WhatAmI::Router, WhatAmI::Peer | WhatAmI::Client) | (WhatAmI::Peer, WhatAmI::Client) => {
            Ok((
                Region::South {
                    id: Default::default(),
                    mode: remote_mode,
                },
                Bound::North,
            ))
        }
        (WhatAmI::Router, WhatAmI::Router) | (WhatAmI::Peer, WhatAmI::Peer) => {
            Ok((Region::North, Bound::North))
        }
        (WhatAmI::Peer | WhatAmI::Client, WhatAmI::Router) | (WhatAmI::Client, WhatAmI::Peer) => {
            Ok((Region::North, Bound::South))
        }
        (WhatAmI::Client, WhatAmI::Client) => {
            bail!("North-north client-client configuration (invalid)")
        }
    }
}

#[allow(clippy::incompatible_msrv)]
fn is_match(filter: Option<&[GatewayFiltersConf]>, peer: &TransportPeer) -> bool {
    filter.is_none_or(|filters| {
        filters.iter().any(|filter| {
            let value = filter
                .zids
                .as_ref()
                .is_none_or(|zid| zid.contains(&peer.zid.into()))
                && filter.interfaces.as_ref().is_none_or(|ifaces| {
                    peer.links
                        .iter()
                        .flat_map(|link| {
                            link.interfaces
                                .iter()
                                .map(|iface| Interface(iface.to_owned()))
                        })
                        .all(|iface| ifaces.contains(&iface))
                })
                && filter
                    .modes
                    .as_ref()
                    .is_none_or(|mode| mode.matches(peer.whatami))
                && filter.region_names.as_ref().is_none_or(|region_names| {
                    peer.region_name
                        .as_ref()
                        .is_some_and(|region_name| region_names.iter().any(|n| n == region_name))
                });

            if filter.negated {
                !value
            } else {
                value
            }
        })
    })
}
