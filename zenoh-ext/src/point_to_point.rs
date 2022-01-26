//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use async_std::io::prelude::WriteExt;
use async_std::net::{TcpListener, TcpStream, ToSocketAddrs};
use async_std::pin::Pin;
use async_std::task::{Context, Poll};
use futures::stream::SelectAll;
use futures::{select, AsyncReadExt, Stream, StreamExt};
use futures_lite::FutureExt;
use std::future::Future;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::str::FromStr;
use zenoh::net::link::tcp::LocatorTcp;
use zenoh::prelude::{KeyExpr, Sample, ZFuture};
use zenoh::queryable::Queryable;
use zenoh_core::{bail, zerror};
use zenoh_util::core::zresult::ZResult;

use crate::session_ext::SessionRef;

async fn get_tcp_addr(address: &LocatorTcp) -> ZResult<SocketAddr> {
    match address {
        LocatorTcp::SocketAddr(addr) => Ok(*addr),
        LocatorTcp::DnsName(addr) => match addr.to_socket_addrs().await {
            Ok(mut addr_iter) => {
                if let Some(addr) = addr_iter.next() {
                    Ok(addr)
                } else {
                    bail!("Couldn't resolve TCP locator address: {}", addr);
                }
            }
            Err(e) => {
                bail!("{}: {}", e, addr);
            }
        },
    }
}

async fn read_msg(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut len = [0_u8, 0_u8];
    stream.read_exact(&mut len).await?;
    let mut data = vec![0_u8; u16::from_le_bytes(len).into()];
    stream.read_exact(&mut data).await?;
    Ok(data)
}

#[derive(Clone)]
pub struct PointToPointServerBuilder<'a, 'b> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) ptp_key_expr: KeyExpr<'b>,
    pub(crate) locator: LocatorTcp,
}

impl<'a, 'b> PointToPointServerBuilder<'a, 'b> {
    pub(crate) fn new(
        session: SessionRef<'a>,
        ptp_key_expr: KeyExpr<'b>,
    ) -> PointToPointServerBuilder<'a, 'b> {
        PointToPointServerBuilder {
            session,
            ptp_key_expr,
            locator: LocatorTcp::from_str("0.0.0.0:7557").unwrap(),
        }
    }

    pub fn listener(mut self, locator: LocatorTcp) -> Self {
        self.locator = locator;
        self
    }
}

impl<'a, 'b> Future for PointToPointServerBuilder<'a, 'b> {
    type Output = ZResult<PointToPointServer<'a>>;

    #[inline]
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Box::pin(PointToPointServer::new(self.clone())).poll(_cx)
    }
}

impl<'a, 'b> ZFuture for PointToPointServerBuilder<'a, 'b> {
    #[inline]
    fn wait(self) -> ZResult<PointToPointServer<'a>> {
        async_std::task::block_on(PointToPointServer::new(self))
    }
}

struct PTPStream {
    stream: Option<TcpStream>,
    #[allow(clippy::type_complexity)]
    fut: Option<Pin<Box<dyn Future<Output = std::io::Result<Vec<u8>>> + Send + Sync>>>,
}

impl PTPStream {
    fn new(stream: TcpStream) -> Self {
        PTPStream {
            stream: Some(stream),
            fut: None,
        }
    }
}

impl Stream for PTPStream {
    type Item = (Vec<u8>, PointToPointChannel);
    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::option::Option<<Self as futures_lite::Stream>::Item>> {
        if self.stream.is_none() {
            return Poll::Pending;
        }
        if self.fut.is_none() {
            let mut stream = self.stream.as_ref().unwrap().clone();
            self.fut = Some(Box::pin(async move { read_msg(&mut stream).await }));
        }
        self.fut.as_mut().unwrap().poll(cx).map(|m| {
            self.fut = None;
            m.ok().map(|m| {
                (
                    m,
                    PointToPointChannel {
                        stream: self.stream.as_ref().unwrap().clone(),
                    },
                )
            })
        })
    }
}

pub struct PointToPointServer<'a> {
    _qabl: Queryable<'a>,
    stream: futures::stream::SelectAll<PTPStream>,
    accept: TcpListener,
}

impl<'a> PointToPointServer<'a> {
    pub async fn recv(&mut self) -> std::io::Result<(Vec<u8>, PointToPointChannel)> {
        use futures::FutureExt;
        loop {
            select!(
                stream = self.accept.accept().fuse() => self.stream.push(PTPStream::new(stream?.0)),
                msg = self.stream.next() => if let Some(msg) = msg { return Ok(msg) },
            )
        }
    }

    pub async fn accept(&mut self) -> std::io::Result<PointToPointChannel> {
        let stream = self.accept.accept().await?.0;
        self.stream.push(PTPStream::new(stream.clone()));
        Ok(PointToPointChannel { stream })
    }

    async fn new(conf: PointToPointServerBuilder<'a, '_>) -> ZResult<PointToPointServer<'a>> {
        use zenoh::prelude::EntityFactory;

        let addr = get_tcp_addr(&conf.locator).await?;
        let socket = TcpListener::bind(addr)
            .await
            .map_err(|e| zerror!("Can not create a new TCP listener on {}: {}", addr, e))?;

        let local_addr = socket
            .local_addr()
            .map_err(|e| zerror!("Can not create a new TCP listener on {}: {}", addr, e))?;

        let mut qabl = match conf.session.clone() {
            SessionRef::Borrow(session) => session.queryable(&conf.ptp_key_expr).await?,
            SessionRef::Shared(session) => session.queryable(&conf.ptp_key_expr).await?,
        };

        async_std::task::spawn({
            let default_ipv4 = Ipv4Addr::new(0, 0, 0, 0);
            let default_ipv6 = Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0);

            let mut receiver = qabl.receiver().clone();
            let key_expr = conf.ptp_key_expr.to_owned();
            async move {
                while let Some(query) = receiver.next().await {
                    let locators = if local_addr.ip() == default_ipv4 {
                        match zenoh_util::net::get_local_addresses() {
                            Ok(ipaddrs) => ipaddrs
                                .into_iter()
                                .filter_map(|ipaddr| {
                                    (!ipaddr.is_loopback()
                                        && !ipaddr.is_multicast()
                                        && ipaddr.is_ipv4())
                                    .then(|| SocketAddr::new(ipaddr, local_addr.port()).to_string())
                                })
                                .collect(),
                            Err(err) => {
                                log::error!("Unable to get local addresses : {}", err);
                                vec![]
                            }
                        }
                    } else if local_addr.ip() == default_ipv6 {
                        match zenoh_util::net::get_local_addresses() {
                            Ok(ipaddrs) => ipaddrs
                                .into_iter()
                                .filter_map(|ipaddr| {
                                    (!ipaddr.is_loopback()
                                        && !ipaddr.is_multicast()
                                        && ipaddr.is_ipv6())
                                    .then(|| SocketAddr::new(ipaddr, local_addr.port()).to_string())
                                })
                                .collect(),
                            Err(err) => {
                                log::error!("Unable to get local addresses : {}", err);
                                vec![]
                            }
                        }
                    } else {
                        vec![local_addr.to_string()]
                    };

                    query.reply(Sample::new(&key_expr, locators.join(";")));
                }
            }
        });

        let mut streams = SelectAll::new();
        streams.push(PTPStream {
            stream: None,
            fut: None,
        });
        let responder = PointToPointServer {
            _qabl: qabl,
            stream: streams,
            accept: socket,
        };

        Ok(responder)
    }
}

pub struct PointToPointChannel {
    stream: TcpStream,
}

impl PointToPointChannel {
    async fn new(
        session: SessionRef<'_>,
        ptp_key_expr: KeyExpr<'_>,
    ) -> ZResult<PointToPointChannel> {
        let mut replies = session.get(&ptp_key_expr).await?;
        if let Some(reply) = replies.next().await {
            for addr in reply.data.value.to_string().split(';') {
                if let Ok(stream) = TcpStream::connect(addr).await {
                    return Ok(PointToPointChannel { stream });
                }
            }
            bail!(
                "Unable to connect to none of {}",
                reply.data.value.to_string()
            )
        } else {
            bail!("Unable to find point to point server on {}", ptp_key_expr)
        }
    }

    pub async fn recv(&mut self) -> std::io::Result<Vec<u8>> {
        read_msg(&mut self.stream).await
    }

    pub async fn send(&mut self, msg: &[u8]) -> std::io::Result<()> {
        self.stream
            .write_all(&(msg.len() as u16).to_le_bytes())
            .await?;
        self.stream.write_all(msg).await
    }
}

pub struct PointToPointChannelBuilder<'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) ptp_key_expr: KeyExpr<'a>,
    pub(crate) future:
        Option<Pin<Box<dyn Future<Output = ZResult<PointToPointChannel>> + Send + Sync + 'a>>>,
}

impl<'a, 'b> PointToPointChannelBuilder<'a> {
    pub(crate) fn new(
        session: SessionRef<'a>,
        ptp_key_expr: KeyExpr<'a>,
    ) -> PointToPointChannelBuilder<'a> {
        PointToPointChannelBuilder {
            session,
            ptp_key_expr,
            future: None,
        }
    }
}

impl<'a> Future for PointToPointChannelBuilder<'a> {
    type Output = ZResult<PointToPointChannel>;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.future.is_none() {
            self.future = Some(Box::pin(PointToPointChannel::new(
                self.session.clone(),
                self.ptp_key_expr.clone(),
            )));
        }
        self.future.as_mut().unwrap().poll(_cx)
    }
}

impl<'a> ZFuture for PointToPointChannelBuilder<'a> {
    #[inline]
    fn wait(self) -> ZResult<PointToPointChannel> {
        async_std::task::block_on(async { self.await })
    }
}
