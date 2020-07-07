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
use std::borrow::Cow;
use log::{debug, warn};


pub struct Workspace {
    session: Session,
    prefix: Path
}


impl Workspace {

    pub(crate) async fn new(session: Session, prefix: Option<Path>) -> ZResult<Workspace> {
        Ok(Workspace { session, prefix: prefix.unwrap_or("/".into()) })
    }

    fn to_absolute_path<'a>(&self, p: &'a Path) -> Cow<'a, Path> {
        if p.is_relative() {
            Cow::Owned(p.with_prefix(&self.prefix))
        } else {
            Cow::Borrowed(p)
        }
    }

    fn to_absolute_pathexpr<'a>(&self, p: &'a PathExpr) -> Cow<'a, PathExpr> {
        if p.is_relative() {
            Cow::Owned(p.with_prefix(&self.prefix))
        } else {
            Cow::Borrowed(p)
        }
    }

    fn to_absolute_selector<'a>(&self, s: &'a Selector) -> Cow<'a, Selector> {
        if s.is_relative() {
            Cow::Owned(s.with_prefix(&self.prefix))
        } else {
            Cow::Borrowed(s)
        }
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
        debug!("write to {:?}", self.path_to_reskey(path));
        self.session.write(&self.path_to_reskey(path), value.into()).await
    }



}