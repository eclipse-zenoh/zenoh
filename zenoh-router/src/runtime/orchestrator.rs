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
use log::{warn, trace};
use zenoh_util::core::ZResult;
use zenoh_protocol::link::Locator;
use zenoh_protocol::session::{Session, SessionManager};

pub struct SessionOrchestrator {
    manager: SessionManager,
    sessions: Vec<Session>,
}

impl SessionOrchestrator {

    pub fn new(manager: SessionManager) -> SessionOrchestrator {
        SessionOrchestrator {
            manager,
            sessions: vec![],
        }
    }

    pub async fn add_acceptor(&self, locator: &Locator) -> ZResult<()> {
        trace!("SessionOrchestrator::add_acceptor({})", locator);
        self.manager.add_locator(locator).await
    }

    pub async fn del_acceptor(&self, locator: &Locator) -> ZResult<()> {
        trace!("SessionOrchestrator::del_acceptor({})", locator);
        self.manager.del_locator(locator).await
    }

    pub async fn open_session(&mut self, locator: &Locator) -> ZResult<()> {
        trace!("SessionOrchestrator::open_session({})", locator);
        match self.manager.open_session(locator, &None).await {
            Ok(session) => {self.sessions.push(session); Ok(())}
            Err(err) => {Err(err)}
        }
    }

    pub async fn add_peer(&mut self, locator: &Locator) {
        trace!("SessionOrchestrator::add_peer({})", locator);
        match self.manager.open_session(locator, &None).await {
            Ok(session) => {self.sessions.push(session);}
            Err(_) => {
                warn!("Unable to open session to ({})", locator);
                // @TODO retry strategy
            } 
        }
    }


    pub async fn close(&mut self) -> ZResult<()> {
        trace!("SessionOrchestrator::close())");
        for session in &mut self.sessions {
            session.close().await?;
        }
        Ok(())
    }
}