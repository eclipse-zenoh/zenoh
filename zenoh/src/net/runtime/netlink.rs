use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{channel::mpsc::UnboundedReceiver, Stream, StreamExt};
use rtnetlink::{
    packet_core::{NetlinkMessage, NetlinkPayload},
    packet_route::RouteNetlinkMessage,
    sys::{AsyncSocket, SocketAddr},
};

const RTNLGRP_IPV4_IFADDR: u32 = 5;

pub struct NetlinkMonitor {
    messages: UnboundedReceiver<(NetlinkMessage<RouteNetlinkMessage>, SocketAddr)>,
}

impl NetlinkMonitor {
    pub fn new() -> io::Result<Self> {
        let (mut conn, _handle, messages) = rtnetlink::new_connection()?;

        // TODO Also handle IPv6 changes.
        let groups = nl_mgrp(RTNLGRP_IPV4_IFADDR);

        let addr = SocketAddr::new(0, groups);
        conn.socket_mut().socket_mut().bind(&addr)?;

        tokio::spawn(conn);

        Ok(Self { messages })
    }
}

impl Stream for NetlinkMonitor {
    type Item = RouteNetlinkMessage;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.messages.poll_next_unpin(cx) {
            Poll::Ready(Some((msg, _))) => match msg.payload {
                NetlinkPayload::InnerMessage(msg) => Poll::Ready(Some(msg)),
                _ => Poll::Pending,
            },
            Poll::Ready(_) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.messages.size_hint()
    }
}

const fn nl_mgrp(group: u32) -> u32 {
    if group > 31 {
        panic!("use netlink_sys::Socket::add_membership() for this group");
    }
    if group == 0 {
        0
    } else {
        1 << (group - 1)
    }
}
