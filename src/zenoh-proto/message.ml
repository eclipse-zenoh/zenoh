(*
 * Copyright (c) 2017, 2020 ADLINK Technology Inc.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Eclipse Public License 2.0 which is available at
 * http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
 * which is available at https://www.apache.org/licenses/LICENSE-2.0.
 *
 * SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
 *
 * Contributors:
 *   ADLINK zenoh team, <zenoh@adlink-labs.tech>
 *)
open Apero
open Apero_net

module MessageId = struct
  let scoutId = char_of_int 0x01
  let helloId = char_of_int 0x02

  let openId = char_of_int 0x03
  let acceptId = char_of_int 0x04
  let closeId = char_of_int 0x05

  let declareId = char_of_int 0x06

  let cdataId = char_of_int 0x07
  let sdataId = char_of_int 0x1A
  let bdataId = char_of_int 0x08
  let wdataId = char_of_int 0x09

  let queryId = char_of_int 0x0a
  let pullId = char_of_int 0x0b

  let pingPongId = char_of_int 0x0c

  let synchId = char_of_int 0x0e
  let ackNackId = char_of_int 0x0f

  let keepAliveId = char_of_int 0x10

  let conduitCloseId = char_of_int 0x11
  let fragmetsId = char_of_int 0x12
  let rSpaceId = char_of_int 0x18

  let conduitId = char_of_int 0x13
  let migrateId = char_of_int 0x14

  let sdeltaDataId = char_of_int 0x15
  let bdeltaDataId = char_of_int 0x16
  let wdeltaDataId = char_of_int 0x17

  let replyId = char_of_int 0x19
end

module Flags = struct
  let sFlag = char_of_int 0x20
  let mFlag = char_of_int 0x20
  let pFlag = char_of_int 0x20

  let rFlag = char_of_int 0x40
  let nFlag = char_of_int 0x40
  let cFlag = char_of_int 0x40
  let eFlag = char_of_int 0x40

  let aFlag = char_of_int 0x80
  let uFlag = char_of_int 0x80

  let zFlag = char_of_int 0x80
  let lFlag = char_of_int 0x20
  let hFlag = char_of_int 0x40

  let gFlag = char_of_int 0x80
  let iFlag = char_of_int 0x20
  let fFlag = char_of_int 0x80
  let oFlag = char_of_int 0x20

  let midMask = char_of_int 0x1f
  let hFlagMask = char_of_int 0xe0

  let hasFlag h f =  (int_of_char h) land (int_of_char f) <> 0
  let mid h =  char_of_int @@ (int_of_char h) land  (int_of_char midMask)
  let flags h = char_of_int @@ (int_of_char h) land  (int_of_char hFlagMask)
  let mid_len = 5

end

module ScoutFlags = struct
  let scoutBroker = 1L
  let scoutDurability = 2L
  let scoutPeer = 4L
  let scoutClient = 8L

  let hasFlag m f = Vle.logand m f <> 0L
end

module DeclarationId = struct
  let resourceDeclId = char_of_int 0x01
  let publisherDeclId = char_of_int 0x02
  let subscriberDeclId = char_of_int 0x03
  let selectionDeclId = char_of_int 0x04
  let bindingDeclId = char_of_int 0x05
  let commitDeclId = char_of_int 0x06
  let resultDeclId = char_of_int 0x07
  let forgetResourceDeclId = char_of_int 0x08
  let forgetPublisherDeclId = char_of_int 0x09
  let forgetSubscriberDeclId = char_of_int 0x0a
  let forgetSelectionDeclId = char_of_int 0x0b
  let storageDeclId = char_of_int 0x0c
  let forgetStorageDeclId = char_of_int 0x0d
  let evalDeclId = char_of_int 0x0e
  let forgetEvalDeclId = char_of_int 0x0f
end

module SubscriptionModeId = struct
  let pushModeId = char_of_int 0x01
  let pullModeId = char_of_int 0x02
  let periodicPushModeId = char_of_int 0x03
  let periodicPullModeId = char_of_int 0x04
end

module TemporalProperty = struct
  type t = {
    origin : Vle.t;
    period : Vle.t;
    duration : Vle.t;
  }

  let create origin period duration = {origin; period; duration}
  let origin p = p.origin
  let period p = p.period
  let duration p = p.duration

end

module SubscriptionMode = struct
  type t =
    | PushMode
    | PullMode
    | PeriodicPushMode of TemporalProperty.t
    | PeriodicPullMode of TemporalProperty.t

  let push_mode = PushMode
  let pull_mode = PullMode
  let periodic_push tp = PeriodicPushMode tp
  let periodic_pull tp = PeriodicPullMode tp

  let id = function
    | PushMode -> SubscriptionModeId.pushModeId
    | PullMode -> SubscriptionModeId.pullModeId
    | PeriodicPushMode _ -> SubscriptionModeId.periodicPushModeId
    | PeriodicPullMode _ -> SubscriptionModeId.periodicPullModeId

  let has_temporal_properties id = id = SubscriptionModeId.periodicPullModeId || id = SubscriptionModeId.periodicPushModeId

  let temporal_properties = function
    | PushMode -> None
    | PullMode -> None
    | PeriodicPullMode tp -> Some tp
    | PeriodicPushMode tp -> Some tp

end


module Header = struct
  type t = char
  let mid h =  (int_of_char h) land (int_of_char Flags.midMask)
  let flags h = (int_of_char h) land (int_of_char Flags.hFlagMask)
end

type 'a block = {body : 'a; header : char}
module Block = struct
  let header msg = msg.header
end

open Block

(** TODO: 
    - Add flag global flag G to Resource/Pub/Sub Declarations to indicate a global resource ID  

    - Remove 
     as everything should be a selection. A predicate on a publisher is
      either ignored or used to filter at the source.
    *)
module ResourceDecl = struct
  type body = {
    rid : Vle.t;
    resource : string;
    properties : ZProperty.t list;
  }
  type t = body block

  let create rid resource properties =
    let header = match properties with
      | [] -> DeclarationId.resourceDeclId
      | _ -> char_of_int ((int_of_char DeclarationId.resourceDeclId) lor (int_of_char Flags.pFlag))
    in {header=header; body={rid=rid; resource=resource; properties=properties}}
  let rid resourceDecl = resourceDecl.body.rid
  let resource resourceDecl = resourceDecl.body.resource
  let properties resourceDecl = resourceDecl.body.properties
end


module PublisherDecl = struct
  type body = {
    rid : Vle.t;
    properties : ZProperty.t list;
  }
  type t = body block

  let create rid properties =
    let header = match properties with
      | [] -> DeclarationId.publisherDeclId
      | _ -> char_of_int ((int_of_char DeclarationId.publisherDeclId) lor (int_of_char Flags.pFlag))
    in {header=header; body={rid=rid; properties=properties}}
  let rid publisherDecl = publisherDecl.body.rid
  let properties publisherDecl = publisherDecl.body.properties
end

module SubscriberDecl = struct
  type body = {
    rid : Vle.t;
    mode : SubscriptionMode.t;
    properties : ZProperty.t list;
  }
  type t = body block

  let create rid mode properties =
    let header = match properties with
      | [] -> DeclarationId.subscriberDeclId
      | _ -> char_of_int ((int_of_char DeclarationId.subscriberDeclId) lor (int_of_char Flags.pFlag))
    in {header=header; body={rid=rid; mode=mode; properties=properties}}
  let rid subscriberDecl = subscriberDecl.body.rid
  let mode subscriberDecl = subscriberDecl.body.mode
  let properties subscriberDecl = subscriberDecl.body.properties
end

module SelectionDecl = struct
  type body = {
    sid : Vle.t;
    query : string;
    properties : ZProperty.t list;
  }
  type t = body block

  let create sid query properties global =
    let header = match properties with
      | [] -> (match global with
        | true -> char_of_int ((int_of_char DeclarationId.selectionDeclId) lor (int_of_char Flags.gFlag))
        | false -> DeclarationId.selectionDeclId)
      | _ -> (match global with
        | true -> char_of_int ((int_of_char DeclarationId.selectionDeclId) lor (int_of_char Flags.pFlag) lor (int_of_char Flags.gFlag))
        | false -> char_of_int ((int_of_char DeclarationId.selectionDeclId) lor (int_of_char Flags.pFlag)))
    in
    {header=header; body={sid=sid; query=query; properties=properties}}
  let sid selectionDecl = selectionDecl.body.sid
  let query selectionDecl = selectionDecl.body.query
  let properties selectionDecl = selectionDecl.body.properties
  let global selectionDecl = ((int_of_char selectionDecl.header) land (int_of_char Flags.gFlag)) <> 0
end

module BindingDecl = struct
  type body = {
    old_id : Vle.t;
    new_id : Vle.t;
  }
  type t = body block

  let create old_id new_id global =
    let header = match global with
      | false -> DeclarationId.bindingDeclId
      | true -> char_of_int ((int_of_char DeclarationId.bindingDeclId) lor (int_of_char Flags.gFlag))
    in {header; body={old_id; new_id}}
  let old_id bd = bd.body.old_id
  let new_id bd = bd.body.new_id
  let global selectionDecl = ((int_of_char selectionDecl.header) land (int_of_char Flags.gFlag)) <> 0
end

module CommitDecl = struct
  type body = {
    commit_id : char;
  }
  type t = body block

  let create id = {header=DeclarationId.commitDeclId; body={commit_id=id}}
  let commit_id cd = cd.body.commit_id
end

module ResultDecl = struct
  type body = {
    commit_id : char;
    status : char;
    id : Vle.t option;
  }
  type t = body block

  let create commit_id status id = {
    header=DeclarationId.resultDeclId;
    body={commit_id;
    status;
    id = if status = char_of_int 0 then None else id}}

  let commit_id d = d.body.commit_id
  let status d = d.body.status
  let id d = d.body.id
end

module ForgetResourceDecl = struct
  type body = {
    rid : Vle.t;
  }
  type t = body block

  let create rid = {header=DeclarationId.forgetResourceDeclId; body={rid=rid}}
  let rid decl = decl.body.rid
end

module ForgetPublisherDecl = struct
  type body = {
    id : Vle.t;
  }
  type t = body block

  let create id = {header=DeclarationId.forgetPublisherDeclId; body={id=id}}
  let id decl = decl.body.id
end

module ForgetSubscriberDecl = struct
  type body = {
    id : Vle.t;
  }
  type t = body block

  let create id = {header=DeclarationId.forgetSubscriberDeclId; body={id=id}}
  let id decl = decl.body.id
end

module ForgetSelectionDecl = struct
  type body = {
    sid : Vle.t;
  }
  type t = body block

  let create sid = {header=DeclarationId.forgetSelectionDeclId; body={sid=sid}}
  let sid decl = decl.body.sid
end

module StorageDecl = struct
  type body = {
    rid : Vle.t;
    properties : ZProperty.t list;
  }
  type t = body block

  let create rid properties =
    let header = match properties with
      | [] -> DeclarationId.storageDeclId
      | _ -> char_of_int ((int_of_char DeclarationId.storageDeclId) lor (int_of_char Flags.pFlag))
    in {header=header; body={rid=rid; properties=properties}}
  let rid storageDecl = storageDecl.body.rid
  let properties storageDecl = storageDecl.body.properties
end

module EvalDecl = struct
  type body = {
    rid : Vle.t;
    properties : ZProperty.t list;
  }
  type t = body block

  let create rid properties =
    let header = match properties with
      | [] -> DeclarationId.evalDeclId
      | _ -> char_of_int ((int_of_char DeclarationId.evalDeclId) lor (int_of_char Flags.pFlag))
    in {header=header; body={rid=rid; properties=properties}}
  let rid evalDecl = evalDecl.body.rid
  let properties evalDecl = evalDecl.body.properties
end

module ForgetStorageDecl = struct
  type body = {
    id : Vle.t;
  }
  type t = body block

  let create id = {header=DeclarationId.forgetStorageDeclId; body={id=id}}
  let id decl = decl.body.id
end

module ForgetEvalDecl = struct
  type body = {
    id : Vle.t;
  }
  type t = body block

  let create id = {header=DeclarationId.forgetEvalDeclId; body={id=id}}
  let id decl = decl.body.id
end

module Declaration = struct
  type t =
    | ResourceDecl of ResourceDecl.t
    | PublisherDecl of PublisherDecl.t
    | SubscriberDecl of SubscriberDecl.t
    | StorageDecl of StorageDecl.t
    | EvalDecl of EvalDecl.t
    | SelectionDecl of SelectionDecl.t
    | BindingDecl of BindingDecl.t
    | CommitDecl of CommitDecl.t
    | ResultDecl of ResultDecl.t
    | ForgetResourceDecl of ForgetResourceDecl.t
    | ForgetPublisherDecl of ForgetPublisherDecl.t
    | ForgetSubscriberDecl of ForgetSubscriberDecl.t
    | ForgetStorageDecl of ForgetStorageDecl.t
    | ForgetEvalDecl of ForgetEvalDecl.t
    | ForgetSelectionDecl of ForgetSelectionDecl.t
end

module Declarations = struct
  type t = Declaration.t list

  let length = List.length
  let empty = []
  let singleton d = [d]
  let add ds d = d::ds
end

(* @AC: A conduit marker should always have a cid, its representation changes, but the id is always there. 
       This should be reflected in the type declaration *)
module ConduitMarker = struct
  type body = {
    id : Vle.t option
  }
  type t = body block

  let create id = 
    match Vle.to_int id with 
      | 1 -> {header=char_of_int ((int_of_char MessageId.conduitId) lor (int_of_char Flags.zFlag)); body={id=None}}
      | 2 -> {header=char_of_int ((int_of_char MessageId.conduitId) lor (int_of_char Flags.lFlag) lor (int_of_char Flags.zFlag)); body={id=None}}
      | 3 -> {header=char_of_int ((int_of_char MessageId.conduitId) lor (int_of_char Flags.hFlag) lor (int_of_char Flags.zFlag)); body={id=None}}
      | 4 -> {header=char_of_int ((int_of_char MessageId.conduitId) lor (int_of_char Flags.hFlag) lor (int_of_char Flags.lFlag) lor (int_of_char Flags.zFlag)); body={id=None}}
      | _ -> {header=MessageId.conduitId; body={id=Some id}}

  let id m = 
    match Flags.(hasFlag m.header zFlag) with 
    | false -> Option.get m.body.id
    | true ->
      let flags = (int_of_char (Flags.flags m.header)) lsr Flags.mid_len in 
      Vle.add (Vle.of_int @@ (flags land 0x3)) Vle.one
end

module Frag = struct
  type body = {
    sn_base : Vle.t;
    n : Vle.t option;
  }
  type t = body block

  let create sn_base n = {header=MessageId.fragmetsId; body={sn_base=sn_base; n=n}}

  let sn_base m = m.body.sn_base
  let n m = m.body.n
end

module RSpace = struct
  type body = {
    id : Vle.t option
  }
  type t = body block

  let create id = 
    match Vle.to_int id with 
      | 0 -> {header=char_of_int ((int_of_char MessageId.rSpaceId) lor (int_of_char Flags.zFlag)); body={id=None}}
      | 1 -> {header=char_of_int ((int_of_char MessageId.rSpaceId) lor (int_of_char Flags.lFlag) lor (int_of_char Flags.zFlag)); body={id=None}}
      | 2 -> {header=char_of_int ((int_of_char MessageId.rSpaceId) lor (int_of_char Flags.hFlag) lor (int_of_char Flags.zFlag)); body={id=None}}
      | 3 -> {header=char_of_int ((int_of_char MessageId.rSpaceId) lor (int_of_char Flags.hFlag) lor (int_of_char Flags.lFlag) lor (int_of_char Flags.zFlag)); body={id=None}}
      | _ -> {header=MessageId.rSpaceId; body={id=Some id}}

  let id m = 
    match Flags.(hasFlag m.header zFlag) with 
    | false -> Option.get m.body.id
    | true ->
      let flags = (int_of_char (Flags.flags m.header)) lsr Flags.mid_len in 
      Vle.of_int @@ (flags land 0x3)
end

type marker =
  | ConduitMarker of ConduitMarker.t
  | Frag of Frag.t
  | RSpace of RSpace.t

module Markers = 
struct
  type t = marker list

  let empty = []
  let with_marker t m  = m :: t
  let with_markers t ms = ms @ t
end

type 'a marked = {mbody : 'a; markers : Markers.t}
module Marked = struct 
  let markers m = m.body.markers
  let with_marker m marker = {header=m.header; body={mbody=m.body.mbody; markers=Markers.with_marker m.body.markers marker}}
  let with_markers m markers = {header=m.header; body={mbody=m.body.mbody; markers=Markers.with_markers m.body.markers markers}}
  let remove_markers m = {header=m.header; body={mbody=m.body.mbody; markers=Markers.empty}}
end
open Marked

 
type 'a reliable = {rbody : 'a; sn : Vle.t}
module Reliable = struct 
  let reliable m = Flags.hasFlag (header m) Flags.rFlag
  let synch m = Flags.hasFlag (header m) Flags.sFlag
  let sn m = m.body.mbody.sn
end


(**
   The SCOUT message can be sent at any point in time to solicit HELLO messages from
   matching parties

   7 6 5 4 3 2 1 0
   +-+-+-+-+-+-+-+-+
   |X|X|P|   SCOUT |
   +-+-+-+-+-+-+-+-+
   ~      Mask     ~
   +-+-+-+-+-+-+-+-+
   ~   Properties  ~
   +-+-+-+-+-+-+-+-+

   b0:     Broker
   b1:     Durability
   b2:     Peer
   b3:     Client
   b4-b13: Reserved
 **)
module Scout = struct
  type body = {
    mask : Vle.t;
    properties : ZProperty.t list;
  }
  type t = body marked block

  let create mask properties =
    let header = match properties with
      | [] -> MessageId.scoutId
      | _ -> char_of_int ((int_of_char MessageId.scoutId) lor (int_of_char Flags.pFlag))
    in {header=header; body={markers=Markers.empty; mbody={mask=mask; properties=properties}}}

  let mask scout = scout.body.mbody.mask
  let properties scout = scout.body.mbody.properties
end

(**
   The SCOUT message can be sent at any point in time to solicit HELLO messages from
   matching parties

     7 6 5 4 3 2 1 0
     +-+-+-+-+-+-+-+-+
     |X|X|P|  HELLO  |
     +-+-+-+-+-+-+-+-+
     ~      Mask     ~
     +-+-+-+-+-+-+-+-+
     ~    Locators   ~
     +-+-+-+-+-+-+-+-+
     ~   Properties  ~
     +-+-+-+-+-+-+-+-+

     The hello message advertise a node and its locators. Locators are expressed as:

     udp/192.168.0.2:1234
     tcp/192.168.0.2:1234
     udp/239.255.255.123:5555
 **)
 
module Hello = struct
  type body = {
    mask : Vle.t;
    locators : Locators.t;
    properties : ZProperty.t list;
  }
  type t = body marked block

  let create mask locators properties =
    let header =
      let pflag = match properties with | [] -> 0 | _ -> int_of_char Flags.pFlag in
      let mid = int_of_char MessageId.helloId in
      char_of_int @@ pflag lor mid in
    {header=header; body={markers=Markers.empty; mbody={mask=mask; locators=locators; properties=properties}}}

  let mask hello = hello.body.mbody.mask
  let locators hello = hello.body.mbody.locators
  let properties hello = hello.body.mbody.properties
  let to_string h =
    Printf.sprintf "Hello:[header: %d, mask: %Ld, locators: %s]" (int_of_char @@ h.header) (mask h) (Locators.to_string (locators h))
end

(**
      7 6 5 4 3 2 1 0
     +-+-+-+-+-+-+-+-+
     |X|X|P|  OPEN   |
     +-------+-------+
     | VMaj  | VMin  |  -- Protocol Version VMaj.VMin
     +-------+-------+
     ~      PID      ~
     +---------------+
     ~ lease_period  ~ -- Expressed in 100s of msec
     +---------------+
     ~    Locators   ~ -- Locators (e.g. multicast) not known due to asymetric knowledge (i.e. HELLO not seen)
     +---------------+
     |  Properties   |
     +---------------+

     - P = 1 => Properties are present and these are used to define (1) SeqLen (2 bytes by default),
     (2) Authorisation, (3) Protocol propertoes such as Commit behaviour, (4) Vendor ID,  etc...

     @TODO: Do we need to add locators for this peer? For client to-broker the client
           always communicates to the broker provided locators
**)
module Open = struct
  type body = {
    version : char;
    pid : Abuf.t;
    lease : Vle.t;
    locators : Locators.t;
    properties : ZProperty.t list;
  }
  type t = body marked block

  let create version pid lease locators properties =
    let header =
      let pflag = match properties with | [] -> 0 | _ -> int_of_char Flags.pFlag in
      let mid = int_of_char MessageId.openId in
      char_of_int @@ pflag lor mid in
    {header=header; body={markers=Markers.empty; mbody={version=version; pid=pid; lease=lease; locators=locators; properties=properties}}}

  let version open_ = open_.body.mbody.version
  let pid open_ = open_.body.mbody.pid
  let lease open_ = open_.body.mbody.lease
  let locators open_ = open_.body.mbody.locators
  let properties open_ = open_.body.mbody.properties
end

(**
      7 6 5 4 3 2 1 0
     +-+-+-+-+-+-+-+-+
     |X|X|P|  ACCEPT |
     +---------------+
     ~     OPID      ~  -- PID of the sender of the OPEN
     +---------------+  -- PID of the "responder" to the OPEN, i.e. accepting entity.
     ~     APID      ~
     +---------------+
     ~ lease_period  ~ -- Expressed in 100s of msec
     +---------------+
     |  Properties   |
     +---------------+
 **)
module Accept = struct
  type body = {
    opid : Abuf.t;
    apid : Abuf.t;
    lease : Vle.t;
    properties : ZProperty.t list;
  }
  type t = body marked block

  let create opid apid lease properties =
    let header =
      let pflag = match properties with | [] -> 0 | _ -> int_of_char Flags.pFlag in
      let mid = int_of_char MessageId.acceptId in
      char_of_int @@ pflag lor mid in
    {header=header; body={markers=Markers.empty; mbody={opid=opid; apid=apid; lease=lease; properties=properties;}}}

  let apid accept = accept.body.mbody.apid
  let opid accept = accept.body.mbody.opid
  let lease accept = accept.body.mbody.lease
  let properties accept = accept.body.mbody.properties
end

(**
        7 6 5 4 3 2 1 0
       +-+-+-+-+-+-+-+-+
       |X|X|X|   CLOSE |
       +---------------+
       ~      PID      ~ -- The PID of the entity that wants to close
       +---------------+
       |     Reason    |
       +---------------+

       - The protocol reserves reasons in the set [0, 127] U {255}
 **)
module Close = struct
  type body = {
    pid : Abuf.t;
    reason : char;
  }
  type t = body marked block

  let create pid reason = {header=MessageId.closeId; body={markers=Markers.empty; mbody={pid=pid; reason=reason}}}

  let pid close = close.body.mbody.pid
  let reason close = close.body.mbody.reason
end

(**
        7 6 5 4 3 2 1 0
       +-+-+-+-+-+-+-+-+
       |X|X|X|   K_A   |
       +---------------+
       ~      PID      ~ -- The PID of peer that wants to renew the lease.
       +---------------+
 **)
module KeepAlive = struct
  type body = {
    pid : Abuf.t;
  }
  type t = body marked block

  let create pid = {header=MessageId.keepAliveId; body={markers=Markers.empty; mbody={pid=pid}}}

  let pid keep_alive = keep_alive.body.mbody.pid
end

(**
        7 6 5 4 3 2 1 0
       +-+-+-+-+-+-+-+-+
       |X|C|S| DECLARE |
       +---------------+
       ~      sn       ~
       +---------------+
       ~ declarations  ~
       +---------------+
 **)
module Declare = struct
  type body = {
    sn : Vle.t;
    declarations : Declarations.t;
  }
  type t = body marked block

  let create (sync, committed) sn declarations =
    let header =
      let sflag = if sync then int_of_char Flags.sFlag  else 0 in
      let cflag = if committed then int_of_char Flags.cFlag  else 0 in
      let mid = int_of_char MessageId.declareId in
      char_of_int @@ sflag lor cflag lor mid in
    {header=header; body={markers=Markers.empty; mbody={sn=sn; declarations=declarations}}}

  let sn declare = declare.body.mbody.sn
  let declarations declare = declare.body.mbody.declarations
  let sync declare = Flags.hasFlag (header declare) Flags.sFlag
  let committed declare = Flags.hasFlag (header declare) Flags.cFlag
end

module WriteData = struct
  type body = {
    resource : string;
    payload: Payload.t;
  }
  type t = body reliable marked block

  let create (s, r) sn resource payload =
    let header  =
      let sflag =  if s then int_of_char Flags.sFlag  else 0 in
      let rflag =  if r then int_of_char Flags.rFlag  else 0 in
      let mid = int_of_char MessageId.wdataId in
      char_of_int @@ sflag lor rflag lor mid in
    { header; body={markers=Markers.empty; mbody={sn; rbody={resource; payload}}}}

  let resource d = d.body.mbody.rbody.resource
  let payload d = d.body.mbody.rbody.payload
  let with_sn d nsn = {d with body = {d.body with mbody = {d.body.mbody with sn = nsn}}}
end

module CompactData = struct
  type body = {
    id : Vle.t;
    prid : Vle.t option;
    payload: Payload.t;
  }
  type t = body reliable marked block

  let create (s, r) sn id prid payload =
    let header  =
      let sflag =  if s then int_of_char Flags.sFlag  else 0 in
      let rflag =  if r then int_of_char Flags.rFlag  else 0 in
      let aflag = match prid with | None -> 0 | _ -> int_of_char Flags.aFlag in
      let mid = int_of_char MessageId.cdataId in
      char_of_int @@ sflag lor rflag lor aflag lor mid in
    { header; body={markers=Markers.empty; mbody={sn; rbody={id; prid; payload}}}}

  let id d = d.body.mbody.rbody.id
  let prid d = d.body.mbody.rbody.prid
  let payload d = d.body.mbody.rbody.payload
  let with_sn d nsn = {d with body = {d.body with mbody = {d.body.mbody with sn = nsn}}}
  let with_id d id = {d with body = {d.body with mbody = {d.body.mbody with rbody = {d.body.mbody.rbody with id = id}}}}
end

module StreamData = struct
  type body = {
    id : Vle.t;
    payload: Payload.t;
  }
  type t = body reliable marked block

  let create (s, r) sn id payload =
    let header  =
      let sflag =  if s then int_of_char Flags.sFlag  else 0 in
      let rflag =  if r then int_of_char Flags.rFlag  else 0 in      
      let mid = int_of_char MessageId.sdataId in
      char_of_int @@ sflag lor rflag lor mid in
    { header; body={markers=Markers.empty; mbody={sn; rbody={id; payload}}}}

  let id d = d.body.mbody.rbody.id  
  let payload d = d.body.mbody.rbody.payload
  let with_sn d nsn = {d with body = {d.body with mbody = {d.body.mbody with sn = nsn}}}
  let with_id d id = {d with body = {d.body with mbody = {d.body.mbody with rbody = {d.body.mbody.rbody with id = id}}}}
end

(**   
     7 6 5 4 3 2 1 0
     +-+-+-+-+-+-+-+-+
     |X|X|P| BSTRDATA|
     +-+-+-+-+-+-+-+-+
     ~       SN      ~
     +-+-+-+-+-+-+-+-+
     ~      id       ~
     +-+-+-+-+-+-+-+-+
     ~   [payload]   ~
     +-+-+-+-+-+-+-+-+
     
 **)
module BatchedStreamData = struct
  type body = {
    id : Vle.t;
    payload: Payload.t list;
  }
  type t = body reliable marked block

  let create (s, r) sn id payload =
    let header  =
      let sflag =  if s then int_of_char Flags.sFlag  else 0 in
      let rflag =  if r then int_of_char Flags.rFlag  else 0 in      
      let mid = int_of_char MessageId.bdataId in
      char_of_int @@ sflag lor rflag lor mid in
    { header; body={markers=Markers.empty; mbody={sn; rbody={id; payload}}}}

  let id d = d.body.mbody.rbody.id  
  let payload d = d.body.mbody.rbody.payload
  let with_sn d nsn = {d with body = {d.body with mbody = {d.body.mbody with sn = nsn}}}
  let with_id d id = {d with body = {d.body with mbody = {d.body.mbody with rbody = {d.body.mbody.rbody with id = id}}}}
end

module Synch = struct
  type body = {
    sn : Vle.t;
    count : Vle.t option
  }
  type t = body marked block

  let create (s, r) sn count =
    let header =
      let uflag = match count with | None -> 0 | _ -> int_of_char Flags.uFlag in
      let rflag = if r then int_of_char Flags.rFlag else 0 in
      let sflag = if s then int_of_char Flags.sFlag else 0 in
      let mid = int_of_char MessageId.synchId in
      char_of_int @@ uflag lor rflag lor sflag lor mid
    in { header; body={markers=Markers.empty; mbody={sn; count}}}

  let sn s = s.body.mbody.sn
  let count s = s.body.mbody.count
end

module AckNack = struct
  type body = {
    sn : Vle.t;
    mask :Vle.t option
  }
  type t = body marked block

  let create sn mask =
    let header =
      let mflag = match mask with | None -> 0 | _ -> int_of_char Flags.mFlag in
      let mid = int_of_char MessageId.ackNackId in
      char_of_int @@ mflag lor mid
    in  { header; body={markers=Markers.empty; mbody={sn; mask}}}

  let sn a = a.body.mbody.sn
  let mask a = a.body.mbody.mask
end

module Migrate = struct
  type body = {
    ocid : Vle.t;
    id : Vle.t option;
    rch_last_sn : Vle.t;
    bech_last_sn : Vle.t;
  }
  type t = body marked block

  let create ocid id rch_last_sn bech_last_sn =
    let header  =
      let iflag = match id with | None -> 0 | _ -> int_of_char Flags.iFlag in
      let mid = int_of_char MessageId.migrateId in
      char_of_int @@ iflag lor mid in
    { header; body={markers=Markers.empty; mbody={ocid; id; rch_last_sn; bech_last_sn}}}
    
  let ocid m = m.body.mbody.ocid
  let id m = m.body.mbody.id
  let rch_last_sn m = m.body.mbody.rch_last_sn
  let bech_last_sn m = m.body.mbody.bech_last_sn
end

module Query = struct
  type body = {
    pid : Abuf.t;
    qid : Vle.t;
    resource : string;
    predicate : string;
    properties : ZProperty.t list;
  }
  type t = body marked block

  let create pid qid resource predicate properties =
    let header = 
      let pFlag = match properties with | [] -> 0 | _ -> int_of_char Flags.pFlag in
      let mid = int_of_char MessageId.queryId in
      char_of_int @@ pFlag lor mid in
    { header; body={markers=Markers.empty; mbody={pid; qid; resource; predicate; properties;}}}

  let pid q = q.body.mbody.pid
  let qid q = q.body.mbody.qid
  let resource q = q.body.mbody.resource
  let predicate q = q.body.mbody.predicate
  let properties q = q.body.mbody.properties
end

module Reply = struct
  type body = {
    qpid : Abuf.t;
    qid : Vle.t;
    value : (Abuf.t * Vle.t * string * Payload.t) option;
  }
  type t = body marked block
  type source = | Storage | Eval

  let create qpid qid source value =
  (* (stoid, rsn, resource, payload) *)
    let header = 
      let fFlag = match value with | None -> 0 | _ -> int_of_char Flags.fFlag in
      let eFlag = match source with | Storage -> 0 | _ -> int_of_char Flags.eFlag in
      let mid = int_of_char MessageId.replyId in
      char_of_int @@ fFlag lor eFlag lor mid in
    { header; body={markers=Markers.empty; mbody={qpid; qid; value;}}}

  let qpid q = q.body.mbody.qpid
  let qid q = q.body.mbody.qid
  let final q = q.body.mbody.value = None
  let value q = q.body.mbody.value
  let source q = match Flags.hasFlag q.header Flags.eFlag with | true -> Eval | false -> Storage
  let stoid q = match q.body.mbody.value with | None -> None | Some (stoid, _, _, _) -> Some stoid
  let rsn q = match q.body.mbody.value with | None -> None | Some (_, rsn, _, _) -> Some rsn
  let resource q = match q.body.mbody.value with | None -> None | Some (_, _, resource, _) -> Some resource
  let payload q = match q.body.mbody.value with | None -> None | Some (_, _, _, payload) -> Some payload
end

module Pull = struct
  type body = {
    sn : Vle.t;
    id : Vle.t;
    max_samples : Vle.t option;
  }
  type t = body marked block

  let create (s, f) sn id max_samples =
    let header  =
      let sflag =  if s then int_of_char Flags.sFlag  else 0 in
      let fflag =  if f then int_of_char Flags.fFlag  else 0 in
      let nflag = match max_samples with | None -> 0 | _ -> int_of_char Flags.nFlag in
      let mid = int_of_char MessageId.pullId in
      char_of_int @@ sflag lor nflag lor fflag lor mid in
    { header; body={markers=Markers.empty; mbody={sn; id; max_samples}}}

  let sn p = p.body.mbody.sn
  let id p = p.body.mbody.id
  let max_samples p = p.body.mbody.max_samples
  let final p = Flags.hasFlag (header p) Flags.fFlag
  let sync p = Flags.hasFlag (header p) Flags.sFlag
end

module PingPong = struct
  type body = {
    hash : Vle.t;
  }
  type t = body marked block

  let create ?pong:(pong=false) hash =
    let header  =
      let oflag =  if pong then int_of_char Flags.oFlag  else 0 in
      let mid = int_of_char MessageId.pingPongId in
      char_of_int @@ oflag lor mid in
    { header; body={markers=Markers.empty; mbody={hash}}}

  let is_pong p = Flags.hasFlag (header p) Flags.oFlag
  let hash p = p.body.mbody.hash
  let to_pong p = {p with header = char_of_int @@ int_of_char (header p) lor int_of_char Flags.oFlag}
end

type t =
  | Scout of Scout.t
  | Hello of Hello.t
  | Open of Open.t
  | Accept of Accept.t
  | Close of Close.t
  | Declare of Declare.t
  | WriteData of WriteData.t
  | CompactData of CompactData.t
  | StreamData of StreamData.t
  | BatchedStreamData of BatchedStreamData.t
  | Synch of Synch.t
  | AckNack of AckNack.t
  | KeepAlive of KeepAlive.t
  | Migrate of Migrate.t
  | Query of Query.t
  | Reply of Reply.t
  | Pull of Pull.t
  | PingPong of PingPong.t

let markers = function
  | Scout s -> markers s
  | Hello h ->  markers h
  | Open o ->  markers o
  | Accept a ->  markers a
  | Close c ->  markers c
  | Declare d ->  markers d
  | WriteData d ->  markers d
  | CompactData d ->  markers d
  | StreamData d ->  markers d
  | BatchedStreamData d ->  markers d
  | Synch s ->  markers s
  | AckNack a ->  markers a
  | KeepAlive a ->  markers a
  | Migrate m ->  markers m
  | Query q ->  markers q
  | Reply r ->  markers r
  | Pull p ->  markers p
  | PingPong p ->  markers p

let with_marker msg marker = match msg with 
  | Scout s -> Scout (with_marker s marker)
  | Hello h ->  Hello (with_marker h marker)
  | Open o ->  Open (with_marker o marker)
  | Accept a ->  Accept (with_marker a marker)
  | Close c ->  Close (with_marker c marker)
  | Declare d ->  Declare (with_marker d marker)
  | WriteData d ->  WriteData (with_marker d marker)
  | CompactData d ->  CompactData (with_marker d marker)
  | StreamData d ->  StreamData (with_marker d marker)
  | BatchedStreamData d -> BatchedStreamData (with_marker d marker)
  | Synch s ->  Synch (with_marker s marker)
  | AckNack a ->  AckNack (with_marker a marker)
  | KeepAlive a ->  KeepAlive (with_marker a marker)
  | Migrate m ->  Migrate (with_marker m marker)
  | Query q ->  Query (with_marker q marker)
  | Reply r ->  Reply (with_marker r marker)
  | Pull p ->  Pull (with_marker p marker)
  | PingPong p ->  PingPong (with_marker p marker)

let with_markers msg markers = match msg with 
  | Scout s -> Scout (with_markers s markers)
  | Hello h ->  Hello (with_markers h markers)
  | Open o ->  Open (with_markers o markers)
  | Accept a ->  Accept (with_markers a markers)
  | Close c ->  Close (with_markers c markers)
  | Declare d ->  Declare (with_markers d markers)
  | WriteData d ->  WriteData (with_markers d markers)
  | CompactData d ->  CompactData (with_markers d markers)
  | StreamData d ->  StreamData (with_markers d markers)
  | BatchedStreamData d ->  BatchedStreamData (with_markers d markers)
  | Synch s ->  Synch (with_markers s markers)
  | AckNack a ->  AckNack (with_markers a markers)
  | KeepAlive a ->  KeepAlive (with_markers a markers)
  | Migrate m ->  Migrate (with_markers m markers)
  | Query q ->  Query (with_markers q markers)
  | Reply r ->  Reply (with_markers r markers)
  | Pull p ->  Pull (with_markers p markers)
  | PingPong p ->  PingPong (with_markers p markers)
  
let remove_markers = function
  | Scout s -> Scout (remove_markers s)
  | Hello h ->  Hello (remove_markers h)
  | Open o ->  Open (remove_markers o)
  | Accept a ->  Accept (remove_markers a)
  | Close c ->  Close (remove_markers c)
  | Declare d ->  Declare (remove_markers d)
  | WriteData d ->  WriteData (remove_markers d)
  | CompactData d ->  CompactData (remove_markers d)
  | StreamData d ->  StreamData (remove_markers d)
  | BatchedStreamData d -> BatchedStreamData (remove_markers d)
  | Synch s ->  Synch (remove_markers s)
  | AckNack a ->  AckNack (remove_markers a)
  | KeepAlive a ->  KeepAlive (remove_markers a)
  | Migrate m ->  Migrate (remove_markers m)
  | Query q ->  Query (remove_markers q)
  | Reply r ->  Reply (remove_markers r)
  | Pull p ->  Pull (remove_markers p)
  | PingPong p ->  PingPong (remove_markers p)

let to_string = function (** This should actually call the to_string on individual messages *)
  | Scout _ -> "Scout"
  | Hello h -> Hello.to_string h
  | Open _ -> "Open"
  | Accept _ -> "Accept"
  | Close _ -> "Close"
  | Declare _ -> "Declare"
  | WriteData _ -> "WriteData"
  | CompactData _ -> "CompactData"
  | StreamData _ -> "StreamData"
  | BatchedStreamData _ -> "BatchedStreamData"
  | Synch _ -> "Synch"
  | AckNack _ -> "AckNack"
  | KeepAlive _ -> "KeepAlive"
  | Migrate _ -> "Migrate"
  | Query _ -> "Query"
  | Reply _ -> "Reply"
  | Pull _ -> "Pull"
  | PingPong _ -> "PingPong"
