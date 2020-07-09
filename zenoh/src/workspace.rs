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
use crate::net::*;
use crate::*;
use log::debug;
use async_std::sync::Receiver;
use async_std::stream::Stream;
use std::convert::TryInto;
use pin_project_lite::pin_project;

pub struct Workspace {
    session: Session,
    prefix: Path
}


impl Workspace {

    pub(crate) async fn new(session: Session, prefix: Option<Path>) -> ZResult<Workspace> {
        Ok(Workspace { session, prefix: prefix.unwrap_or_else(|| "/".try_into().unwrap()) })
    }

    fn path_to_reskey(&self, path: &Path) -> ResKey {
        if path.is_relative() {
            ResKey::from(path.with_prefix(&self.prefix))
        } else {
            ResKey::from(path)
        }
    }

    fn pathexpr_to_reskey(&self, path: &PathExpr) -> ResKey {
        if path.is_relative() {
            ResKey::from(path.with_prefix(&self.prefix))
        } else {
            ResKey::from(path)
        }
    }

    pub async fn put(&self, path: &Path, value: &dyn Value) -> ZResult<()> {
        debug!("put on {:?}", path);
        self.session.write_wo(
            &self.path_to_reskey(path),
            value.into(),
            value.encoding(),
            kind::PUT
        ).await
    }

    pub async fn delete(&self, path: &Path) -> ZResult<()> {
        debug!("delete on {:?}", path);
        self.session.write_wo(
            &self.path_to_reskey(path),
            RBuf::empty(),
            encoding::RAW,
            kind::DELETE
        ).await
    }

    pub async fn get(&self, selector: &Selector) -> ZResult<DataStream> {
        debug!("get on {}", selector);
        let pathexpr = self.pathexpr_to_reskey(&selector.path_expr);

        self.session.query(
            &pathexpr,
            &selector.predicate,
            QueryTarget::default(),
            QueryConsolidation::default()
        ).await
        .map(|receiver| DataStream { receiver })
    }

}





pub struct Data {
    pub path: Path,
    pub value: Box<dyn Value>
}


pin_project! {
    pub struct DataStream {
        #[pin]
        receiver: Receiver<Reply>
    }
}

impl Stream for DataStream {
    type Item = Data;

    #[inline(always)]
    fn poll_next(self: async_std::pin::Pin<&mut Self>, cx: &mut async_std::task::Context) -> async_std::task::Poll<Option<Self::Item>> {
        self.project().receiver.poll_next(cx).map(reply_to_data)
    }
}

fn reply_to_data(reply: Option<Reply>) -> Option<Data> {
    reply.map(|r| Data {
        path: r.data.res_name.try_into().unwrap(),
        value: Box::new(RawValue::from(r.data.payload))
    })
}

