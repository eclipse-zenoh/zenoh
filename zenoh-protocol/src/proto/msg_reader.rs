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
use crate::io::RBuf;
use crate::core::*;
use crate::link::Locator;

use super::msg::*;
use super::decl::*;

use zenoh_util::zerror;
use zenoh_util::core::{ZResult, ZError, ZErrorKind};


impl RBuf {
    pub fn read_session_message(&mut self) -> ZResult<SessionMessage> {
        use super::smsg::id::*;

        let mut attachment = None;
        
        // Read the message
        let (header, body) = loop {
            // Read the header
            let header = self.read()?;

            // Read the body
            match smsg::mid(header) {
                // Frame as first for optimization reasons
                FRAME => {
                    let ch = smsg::has_flag(header, smsg::flag::R);
                    let sn = self.read_zint()?;

                    let payload = if smsg::has_flag(header, smsg::flag::F) {
                        // A fragmented frame is not supposed to be followed by
                        // any other frame in the same batch. Read all the bytes.
                        let mut buffer = RBuf::new();
                        self.drain_into_rbuf(&mut buffer);
                        let is_final = smsg::has_flag(header, smsg::flag::E);

                        FramePayload::Fragment { buffer, is_final }
                    } else {
                        // @TODO: modify the get_pos/set_pos to mark/revert 
                        let mut messages: Vec<ZenohMessage> = Vec::with_capacity(1);
                        loop {
                            let pos = self.get_pos();
                            if let Ok(msg) = self.read_zenoh_message() {
                                messages.push(msg);
                            } else {
                                self.set_pos(pos)?;
                                break
                            }
                        }
                        
                        FramePayload::Messages { messages }
                    };

                    let body = SessionBody::Frame(Frame { ch, sn, payload });
                    break (header, body)
                },
                
                ATTACHMENT => {
                    attachment = Some(self.read_deco_attachment(header)?);
                    continue
                },

                SCOUT => {
                    let pid_replies = smsg::has_flag(header, smsg::flag::I);
                    let what = if smsg::has_flag(header, smsg::flag::W) {
                        Some(self.read_zint()?)
                    } else { 
                        None 
                    }; 
                    let forwarding = smsg::has_flag(header, smsg::flag::T);

                    let body = SessionBody::Scout(Scout { what, pid_replies, forwarding });
                    break (header, body)
                },

                HELLO => {
                    let pid = if smsg::has_flag(header, smsg::flag::I) {
                        Some(self.read_peerid()?)
                    } else { 
                        None
                    };
                    let whatami = if smsg::has_flag(header, smsg::flag::W) {
                        Some(self.read_zint()?)
                    } else { 
                        None
                    };
                    let locators = if smsg::has_flag(header, smsg::flag::L) {
                        Some(self.read_locators()?)
                    } else { 
                        None 
                    };
                    
                    let body = SessionBody::Hello(Hello { pid, whatami, locators });
                    break (header, body)
                },

                OPEN => {
                    let version = self.read()?;
                    let whatami = self.read_zint()?;
                    let pid = self.read_peerid()?;
                    let lease = self.read_zint()?;
                    let initial_sn = self.read_zint()?;
                    let (sn_resolution, locators) = if smsg::has_flag(header, smsg::flag::O) {
                        let options = self.read()?;
                        let sn_resolution = if smsg::has_flag(options, smsg::flag::S) {
                            Some(self.read_zint()?)
                        } else {
                            None
                        };
                        let locators = if smsg::has_flag(options, smsg::flag::L) {
                            Some(self.read_locators()?)
                        } else { 
                            None 
                        };
                        (sn_resolution, locators)
                    } else {
                        (None, None)
                    };

                    let body = SessionBody::Open(Open { version, whatami, pid, lease, initial_sn, sn_resolution, locators });
                    break (header, body)
                },

                ACCEPT => {
                    let whatami = self.read_zint()?;
                    let opid = self.read_peerid()?;
                    let apid = self.read_peerid()?;
                    let initial_sn = self.read_zint()?;
                    let (sn_resolution, lease, locators) = if smsg::has_flag(header, smsg::flag::O) {
                        let options = self.read()?;
                        let sn_resolution = if smsg::has_flag(options, smsg::flag::S) {
                            Some(self.read_zint()?)
                        } else {
                            None
                        };
                        let lease = if smsg::has_flag(options, smsg::flag::D) {
                            Some(self.read_zint()?)
                        } else {
                            None
                        };
                        let locators = if smsg::has_flag(options, smsg::flag::L) {
                            Some(self.read_locators()?)
                        } else { 
                            None 
                        };
                        (sn_resolution, lease, locators)
                    } else {
                        (None, None, None)
                    };

                    let body = SessionBody::Accept(Accept { whatami, opid, apid, initial_sn, sn_resolution, lease, locators });
                    break (header, body)
                },

                CLOSE => {
                    let link_only = smsg::has_flag(header, smsg::flag::K);
                    let pid = if smsg::has_flag(header, smsg::flag::I) {
                        Some(self.read_peerid()?)
                    } else { 
                        None 
                    };
                    let reason = self.read()?;

                    let body = SessionBody::Close(Close { pid, reason, link_only });
                    break (header, body)
                },

                SYNC => {
                    let ch = smsg::has_flag(header, smsg::flag::R);
                    let sn = self.read_zint()?;
                    let count = if smsg::has_flag(header, smsg::flag::C) {
                        Some(self.read_zint()?)
                    } else { 
                        None 
                    };
                    
                    let body = SessionBody::Sync(Sync { ch, sn, count });
                    break (header, body)
                },

                ACK_NACK => {
                    let sn = self.read_zint()?;
                    let mask = if smsg::has_flag(header, smsg::flag::M) {
                        Some(self.read_zint()?)
                    } else { 
                        None 
                    };
                    
                    let body = SessionBody::AckNack(AckNack { sn, mask });
                    break (header, body)
                },

                KEEP_ALIVE => {
                    let pid = if smsg::has_flag(header, smsg::flag::I) {
                        Some(self.read_peerid()?)
                    } else { 
                        None 
                    };
                    
                    let body = SessionBody::KeepAlive(KeepAlive { pid });
                    break (header, body)
                },

                PING_PONG => {
                    let hash = self.read_zint()?;
                    
                    let body = if smsg::has_flag(header, smsg::flag::P) {
                        SessionBody::Ping(Ping { hash })
                    } else {
                        SessionBody::Pong(Pong { hash })
                    };

                    break (header, body)
                },                

                unknown => return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Session message with unknown ID: {}", unknown)
                })
            }
        };

        Ok(SessionMessage { header, body, attachment })        
    }

    pub fn read_zenoh_message(&mut self) -> ZResult<ZenohMessage> {
        use super::zmsg::id::*;

        // Message decorators
        let mut reply_context = None;
        let mut attachment = None;

        // Read the message
        let (header, body, channel) = loop {
            // Read the header
            let header = self.read()?;

            // Read the body
            match zmsg::mid(header) {
                // Message data as first for optimization reasons
                DATA => {
                    let channel = zmsg::has_flag(header, zmsg::flag::R);
                    let key = self.read_reskey(zmsg::has_flag(header, zmsg::flag::K))?;
                    let info = if zmsg::has_flag(header, zmsg::flag::I) {
                        Some(RBuf::from(self.read_bytes_array()?))
                    } else { 
                        None 
                    };
                    let payload = self.read_rbuf()?;

                    let body = ZenohBody::Data(Data { key, info, payload });
                    break (header, body, channel)
                },

                // Decorators
                REPLY_CONTEXT => {
                    reply_context = Some(self.read_deco_reply(header)?);
                    continue
                },

                ATTACHMENT => {
                    attachment = Some(self.read_deco_attachment(header)?);
                    continue
                },

                // Messages
                DECLARE => {
                    let declarations = self.read_declarations()?;

                    let body = ZenohBody::Declare(Declare { declarations });
                    let channel = zmsg::default_channel::DECLARE;
                    break (header, body, channel)
                },

                UNIT => {
                    let channel = zmsg::has_flag(header, zmsg::flag::R);

                    let body = ZenohBody::Unit(Unit {});
                    break (header, body, channel)
                },

                PULL => {
                    let is_final = zmsg::has_flag(header, zmsg::flag::F);
                    let key = self.read_reskey(zmsg::has_flag(header, zmsg::flag::K))?;
                    let pull_id = self.read_zint()?;
                    let max_samples = if zmsg::has_flag(header, zmsg::flag::N) {
                        Some(self.read_zint()?)
                    } else { 
                        None 
                    };

                    let body = ZenohBody::Pull(Pull { key, pull_id, max_samples, is_final });
                    let channel = zmsg::default_channel::PULL;
                    break (header, body, channel)
                },

                QUERY => {
                    let key = self.read_reskey(zmsg::has_flag(header, zmsg::flag::K))?;
                    let predicate = self.read_string()?;
                    let qid = self.read_zint()?;
                    let target = if zmsg::has_flag(header, zmsg::flag::T) {
                        Some(self.read_query_target()?)
                    } else { 
                        None 
                    };
                    let consolidation = self.read_consolidation()?;
                    
                    let body = ZenohBody::Query(Query { key, predicate, qid, target, consolidation });
                    let channel = zmsg::default_channel::QUERY;
                    break (header, body, channel)
                },

                unknown => return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Zenoh message with unknown ID: {}", unknown)
                })
            }
        };

        Ok(ZenohMessage { header, body, channel, reply_context, attachment })
    }

    fn read_deco_attachment(&mut self, header: u8) -> ZResult<Attachment> {
        let encoding = smsg::flags(header);
        let buffer = self.read_rbuf()?;
        Ok(Attachment { encoding, buffer })
    }

    // @TODO: Update the ReplyContext format
    fn read_deco_reply(&mut self, header: u8) -> ZResult<ReplyContext> {
        let is_final = zmsg::has_flag(header, zmsg::flag::F);
        let qid = self.read_zint()?;
        let source_kind = self.read_zint()?;
        let replier_id = if is_final { None } else {
            Some(self.read_peerid()?)
        };
        Ok(ReplyContext{ is_final, qid, source_kind, replier_id })
    }

    pub fn read_datainfo(&mut self) -> ZResult<DataInfo> {
        let header = self.read()?;
        let source_id = if header & zmsg::info_flag::SRCID > 0 {
            Some(self.read_peerid()?)
        } else { None };
        let source_sn = if header & zmsg::info_flag::SRCSN > 0 {
            Some(self.read_zint()?)
        } else { None };
        let fist_broker_id = if header & zmsg::info_flag::BKRID > 0 {
            Some(self.read_peerid()?)
        } else { None };
        let fist_broker_sn = if header & zmsg::info_flag::BKRSN > 0 {
            Some(self.read_zint()?)
        } else { None };
        let timestamp = if header & zmsg::info_flag::TS > 0 {
            Some(self.read_timestamp()?)
        } else { None };
        let kind = if header & zmsg::info_flag::KIND > 0 {
            Some(self.read_zint()?)
        } else { None };
        let encoding = if header & zmsg::info_flag::ENC > 0 {
            Some(self.read_zint()?)
        } else { None };

        Ok(DataInfo { source_id, source_sn, fist_broker_id, fist_broker_sn, timestamp, kind, encoding })
    }

    pub fn read_properties(&mut self) -> ZResult<Vec<Property>> {
        let len = self.read_zint()?;
        let mut vec: Vec<Property> = Vec::new();
        for _ in 0..len {
            vec.push(self.read_property()?);
        }
        Ok(vec)
    }

    fn read_property(&mut self) -> ZResult<Property> {
        let key = self.read_zint()?;
        let value = self.read_bytes_array()?;
        Ok(Property{ key, value })
    }

    fn read_locators(&mut self) -> ZResult<Vec<Locator>> {
        let len = self.read_zint()?;
        let mut vec: Vec<Locator> = Vec::new();
        for _ in 0..len {
            vec.push(self.read_string()?.parse()?);
        }
        Ok(vec)
    }

    fn read_declarations(&mut self) -> ZResult<Vec<Declaration>> {
        let len = self.read_zint()?;
        let mut vec: Vec<Declaration> = Vec::new();
        for _ in 0..len {
            vec.push(self.read_declaration()?);
        }
        Ok(vec)
    }

    fn read_declaration(&mut self) -> ZResult<Declaration> {
        use super::decl::{Declaration::*, id::*};

        macro_rules! read_key_delc {
            ($buf:ident, $header:ident, $type:ident) => {{
                Ok($type{ 
                    key: $buf.read_reskey(zmsg::has_flag($header, zmsg::flag::K))?
                })
            }}
        }

        let header = self.read()?;
        match zmsg::mid(header) {
            RESOURCE => {
                let rid = self.read_zint()?;
                let key = self.read_reskey(zmsg::has_flag(header, zmsg::flag::K))?;
                Ok(Resource{ rid, key })
            }

            FORGET_RESOURCE => {
                let rid = self.read_zint()?;
                Ok(ForgetResource{ rid })
            }

            SUBSCRIBER => {
                let reliability = if zmsg::has_flag(header, zmsg::flag::R) {
                    Reliability::Reliable 
                } else {
                    Reliability::BestEffort
                };
                let key = self.read_reskey(zmsg::has_flag(header, zmsg::flag::K))?;
                let (mode, period) = if zmsg::has_flag(header, zmsg::flag::S) {
                    self.read_submode()?
                } else {
                    (SubMode::Push, None)
                };
                Ok(Subscriber{ key, info: SubInfo { reliability, mode, period } })
            }

            FORGET_SUBSCRIBER => read_key_delc!(self, header, ForgetSubscriber),
            PUBLISHER => read_key_delc!(self, header, Publisher),
            FORGET_PUBLISHER => read_key_delc!(self, header, ForgetPublisher),
            QUERYABLE => read_key_delc!(self, header, Queryable),
            FORGET_QUERYABLE => read_key_delc!(self, header, ForgetQueryable),

            id => panic!("UNEXPECTED ID FOR Declaration: {}", id)   //@TODO: return error
        }
    }

    fn read_submode(&mut self) -> ZResult<(SubMode, Option<Period>)> {
        use super::decl::id::*;
        let mode_flag = self.read()?;
        let mode = match mode_flag & !PERIOD {
            id::MODE_PUSH => SubMode::Push,
            id::MODE_PULL => SubMode::Pull,
            id => panic!("UNEXPECTED ID FOR SubMode: {}", id)   //@TODO: return error
        };
        let period = if mode_flag & PERIOD > 0{
            Some(Period{
                origin:   self.read_zint()?,
                period:   self.read_zint()?,
                duration: self.read_zint()?
            })
        } else {
            None
        };
        Ok((mode, period))
    }

    fn read_reskey(&mut self, is_numeric: bool) -> ZResult<ResKey> {
        let id = self.read_zint()?;
        if is_numeric {
            Ok(ResKey::RId(id))
        } else {
            let s = self.read_string()?;
            if id == NO_RESOURCE_ID {
                Ok(ResKey::RName(s))
            } else {
                Ok(ResKey::RIdWithSuffix(id, s))
            }
        }
    }

    fn read_query_target(&mut self) -> ZResult<QueryTarget> {
        let kind = self.read_zint()?;
        let target = self.read_target()?;
        Ok(QueryTarget{ kind, target })
    }

    fn read_target(&mut self) -> ZResult<Target> {
        let t = self.read_zint()?;
        match t {
            0 => Ok(Target::BestMatching),
            1 => {
                let n = self.read_zint()?;
                Ok(Target::Complete{n})
            },
            2 => Ok(Target::All),
            3 => Ok(Target::None),
            id => panic!("UNEXPECTED ID FOR Target: {}", id)   //@TODO: return error
        }
    }

    fn read_consolidation(&mut self) -> ZResult<QueryConsolidation> {
        match self.read_zint()? {
            0 => Ok(QueryConsolidation::None),
            1 => Ok(QueryConsolidation::LastHop),
            2 => Ok(QueryConsolidation::Incremental),
            id => panic!("UNEXPECTED ID FOR QueryConsolidation: {}", id)   //@TODO: return error
        }
    }

    pub fn read_timestamp(&mut self) -> ZResult<TimeStamp> {
        let time = self.read_zint()?;
        let mut bytes = [0u8; 16];
        self.read_bytes(&mut bytes[..])?;
        let id = uuid::Builder::from_bytes(bytes).build();
        Ok(TimeStamp { time, id })
    }

    fn read_peerid(&mut self) -> ZResult<PeerId> {
        let id = self.read_bytes_array()?;
        Ok(PeerId { id })
    }

}