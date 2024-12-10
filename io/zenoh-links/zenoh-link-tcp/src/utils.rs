//
// Copyright (c) 2024 ZettaScale Technology
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
use zenoh_config::Config as ZenohConfig;
use zenoh_link_commons::{
    tcp::TcpSocketConfig, ConfigurationInspector, BIND_INTERFACE, TCP_SO_RCV_BUF, TCP_SO_SND_BUF,
};
use zenoh_protocol::core::{parameters, Config};
use zenoh_result::{zerror, ZResult};

#[derive(Default, Clone, Copy, Debug)]
pub struct TcpConfigurator;

impl ConfigurationInspector<ZenohConfig> for TcpConfigurator {
    fn inspect_config(&self, config: &ZenohConfig) -> ZResult<String> {
        let mut ps: Vec<(&str, &str)> = vec![];
        let c = config.transport().link().tcp();

        let rx_buffer_size;
        if let Some(size) = c.so_rcvbuf() {
            rx_buffer_size = size.to_string();
            ps.push((TCP_SO_RCV_BUF, &rx_buffer_size));
        }
        let tx_buffer_size;
        if let Some(size) = c.so_sndbuf() {
            tx_buffer_size = size.to_string();
            ps.push((TCP_SO_SND_BUF, &tx_buffer_size));
        }

        Ok(parameters::from_iter(ps.drain(..)))
    }
}

pub(crate) struct TcpLinkConfig<'a> {
    pub(crate) rx_buffer_size: Option<u32>,
    pub(crate) tx_buffer_size: Option<u32>,
    pub(crate) bind_iface: Option<&'a str>,
}

impl<'a> TcpLinkConfig<'a> {
    pub(crate) fn new(config: &'a Config) -> ZResult<Self> {
        let mut tcp_config = Self {
            rx_buffer_size: None,
            tx_buffer_size: None,
            bind_iface: config.get(BIND_INTERFACE),
        };

        if let Some(size) = config.get(TCP_SO_RCV_BUF) {
            tcp_config.rx_buffer_size = Some(
                size.parse()
                    .map_err(|_| zerror!("Unknown TCP read buffer size argument: {}", size))?,
            );
        };
        if let Some(size) = config.get(TCP_SO_SND_BUF) {
            tcp_config.tx_buffer_size = Some(
                size.parse()
                    .map_err(|_| zerror!("Unknown TCP write buffer size argument: {}", size))?,
            );
        };

        Ok(tcp_config)
    }
}

impl<'a> From<TcpLinkConfig<'a>> for TcpSocketConfig<'a> {
    fn from(value: TcpLinkConfig<'a>) -> Self {
        Self::new(value.tx_buffer_size, value.rx_buffer_size, value.bind_iface)
    }
}
