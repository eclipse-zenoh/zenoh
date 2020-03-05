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
open Locator

module MessageId :
sig
  val scoutId : char
  val helloId : char
  val openId : char
  val acceptId : char
  val closeId : char
  val declareId : char
  val cdataId : char
  val sdataId : char
  val bdataId : char
  val wdataId : char
  val queryId : char
  val pullId : char
  val pingPongId : char
  val synchId : char
  val ackNackId : char
  val keepAliveId : char
  val conduitCloseId : char
  val fragmetsId : char
  val conduitId : char
  val rSpaceId : char
  val migrateId : char
  val sdeltaDataId : char
  val bdeltaDataId : char
  val wdeltaDataId : char
  val replyId : char
end

module Flags :
sig
  val sFlag : char
  val mFlag : char
  val pFlag : char
  val rFlag : char
  val nFlag : char
  val cFlag : char
  val eFlag : char
  val aFlag : char
  val uFlag : char
  val zFlag : char
  val lFlag : char
  val hFlag : char
  val gFlag : char
  val iFlag : char
  val fFlag : char
  val oFlag : char
  val midMask : char
  val hFlagMask : char

  val hasFlag : char -> char -> bool
  val mid : char -> char
  val mid_len : int
  val flags : char -> char

end

module ScoutFlags :
sig
  val scoutBroker : Vle.t
  val scoutDurability : Vle.t
  val scoutPeer : Vle.t
  val scoutClient : Vle.t

  val hasFlag : Vle.t -> Vle.t -> bool
end

module DeclarationId :
sig
  val resourceDeclId : char
  val publisherDeclId : char
  val subscriberDeclId : char
  val selectionDeclId : char
  val bindingDeclId : char
  val commitDeclId : char
  val resultDeclId : char
  val forgetResourceDeclId : char
  val forgetPublisherDeclId : char
  val forgetSubscriberDeclId : char
  val forgetSelectionDeclId : char
  val storageDeclId : char
  val forgetStorageDeclId : char
  val evalDeclId : char
  val forgetEvalDeclId : char
end

module SubscriptionModeId :
sig
  val pushModeId : char
  val pullModeId : char
  val periodicPushModeId : char
  val periodicPullModeId : char
end

module TemporalProperty : sig
  type t = {
    origin : Vle.t;
    period : Vle.t;
    duration : Vle.t;
  }

  val create : Vle.t -> Vle.t -> Vle.t -> t
  val origin : t -> Vle.t
  val period : t -> Vle.t
  val duration : t -> Vle.t
end

module SubscriptionMode :
sig

  type t =
      | PushMode
      | PullMode
      | PeriodicPushMode of TemporalProperty.t
      | PeriodicPullMode of TemporalProperty.t

  val push_mode : t
  val pull_mode : t
  val periodic_push : TemporalProperty.t -> t
  val periodic_pull : TemporalProperty.t  -> t

  val id : t -> char

  val has_temporal_properties : char -> bool

  val temporal_properties : t -> TemporalProperty.t option
end


module Header :
sig
  type t = char
  val mid : char -> int
  val flags : char -> int
end

type 'a block
module Block : 
sig
  val header : 'a block -> char
end

module ResourceDecl :
sig
  type body
  type t = body block
  val create : Vle.t -> string -> ZProperty.t list -> t
  val rid : t -> Vle.t
  val resource : t -> string
  val properties : t -> ZProperty.t list
end

module PublisherDecl :
sig
  type body
  type t = body block
  val create : Vle.t -> ZProperty.t list -> t
  val rid : t -> Vle.t
  val properties : t -> ZProperty.t list
end

module  SubscriberDecl :
sig
  type body
  type t = body block
  val create : Vle.t -> SubscriptionMode.t -> ZProperty.t list -> t
  val rid : t -> Vle.t
  val mode : t -> SubscriptionMode.t
  val properties : t -> ZProperty.t list
end

module StorageDecl :
sig
  type body
  type t = body block
  val create : Vle.t -> ZProperty.t list -> t
  val rid : t -> Vle.t
  val properties : t -> ZProperty.t list
end

module EvalDecl :
sig
  type body
  type t = body block
  val create : Vle.t -> ZProperty.t list -> t
  val rid : t -> Vle.t
  val properties : t -> ZProperty.t list
end

module SelectionDecl :
sig
  type body
  type t = body block
  val create : Vle.t -> string -> ZProperty.t list -> bool -> t
  val sid : t -> Vle.t
  val query : t -> string
  val properties : t -> ZProperty.t list
  val global : t -> bool
end

module BindingDecl :
sig
  type body
  type t = body block
  val create : Vle.t -> Vle.t -> bool -> t
  val old_id : t -> Vle.t
  val new_id : t -> Vle.t
  val global : t -> bool
end

module CommitDecl :
sig
  type body
  type t = body block
  val create : char -> t
  val commit_id : t -> char
end

module ResultDecl :
sig
  type body
  type t = body block
  val create : char -> char -> Vle.t option-> t
  val commit_id : t -> char
  val status : t -> char
  val id : t -> Vle.t option
end

module ForgetResourceDecl :
sig
  type body
  type t = body block
  val create : Vle.t -> t
  val rid : t -> Vle.t
end

module ForgetPublisherDecl :
sig
  type body
  type t = body block
  val create : Vle.t -> t
  val id : t -> Vle.t
end

module ForgetSubscriberDecl :
sig
  type body
  type t = body block
  val create : Vle.t -> t
  val id : t -> Vle.t
end

module ForgetStorageDecl :
sig
  type body
  type t = body block
  val create : Vle.t -> t
  val id : t -> Vle.t
end

module ForgetEvalDecl :
sig
  type body
  type t = body block
  val create : Vle.t -> t
  val id : t -> Vle.t
end

module ForgetSelectionDecl :
sig
  type body
  type t = body block
  val create : Vle.t -> t
  val sid : t -> Vle.t
end

(** @AC: Originally the Selections were supposed to be continuous queries. Notice that
    the main difference between a selection and a subscription is that the seleciton was
    supposed to have a query portion. 
    Beside this the difference between a Selection and a Query is that the Selection 
    produces a continuous stream of data.
    
    Selections, arent' currently implemented and the questions is whether we should
    remove them from the protocol alltogether. 

    The matter with selections is that they would require the producer or the broker 
    to be able to interpret the data. This is not necessarily something we want to
    do in zenoh.
*) 

module Declaration :
sig
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

module Declarations : sig
  type t = Declaration.t list

  val length : t -> int
  val empty : t
  val singleton : Declaration.t -> t
  val add : t -> Declaration.t -> t
end

module ConduitMarker :
sig
  type body
  type t = body block
  val create : Vle.t -> t
  val id : t -> Vle.t
end

module Frag :
sig
  type body
  type t = body block
  val create : Vle.t -> Vle.t option -> t
  val sn_base : t -> Vle.t
  val n : t -> Vle.t option
end

module RSpace :
sig
  type body
  type t = body block
  val create : Vle.t -> t
  val id : t -> Vle.t
end 

type marker =
  | ConduitMarker of ConduitMarker.t
  | Frag of Frag.t
  | RSpace of RSpace.t

type 'a marked
module Marked : 
sig
  val markers : 'a marked block -> marker list
  val with_marker : 'a marked block -> marker -> 'a marked block
  val with_markers : 'a marked block -> marker list -> 'a marked block
  val remove_markers : 'a marked block -> 'a marked block
end

type 'a reliable
module Reliable : 
sig
  val reliable : 'a reliable marked block -> bool
  val synch : 'a reliable marked block -> bool
  val sn : 'a reliable marked block -> Vle.t
end

module Scout :
sig
  type body
  type t = body marked block
  val create : Vle.t -> ZProperty.t list -> t
  val mask : t -> Vle.t
  val properties : t -> ZProperty.t list
end

module Hello :
sig
  type body
  type t = body marked block
  val create : Vle.t -> Locators.t -> ZProperty.t list -> t
  val mask : t -> Vle.t
  val locators : t -> Locators.t
  val properties : t -> ZProperty.t list
end

module Open :
sig
  type body
  type t = body marked block
  val create : char -> Abuf.t -> Vle.t -> Locators.t -> ZProperty.t list -> t
  val version : t -> char
  val pid : t -> Abuf.t
  val lease : t -> Vle.t
  val locators : t -> Locators.t
  val properties : t -> ZProperty.t list
end

module Accept :
sig
  type body
  type t = body marked block
  val create : Abuf.t -> Abuf.t -> Vle.t -> ZProperty.t list -> t
  val opid : t -> Abuf.t
  val apid : t -> Abuf.t
  val lease : t -> Vle.t
  val properties : t -> ZProperty.t list
end

module Close :
sig
  type body
  type t = body marked block
  val create : Abuf.t -> char -> t
  val pid : t -> Abuf.t
  val reason : t -> char
end

module KeepAlive :
sig
  type body
  type t = body marked block
  val create : Abuf.t -> t
  val pid : t -> Abuf.t
end

module Declare :
sig
  type body
  type t = body marked block
  val create : (bool * bool) -> Vle.t -> Declaration.t list -> t
  val sn : t -> Vle.t
  val declarations : t -> Declaration.t list
  val sync : t -> bool
  val committed : t -> bool
end

module WriteData :
sig
  type body
  type t = body reliable marked block
  val create : bool * bool -> Vle.t -> string -> Payload.t -> t
  val resource : t -> string
  val payload : t -> Payload.t
  val with_sn : t -> Vle.t -> t
end

module CompactData :
sig
  type body
  type t = body reliable marked block
  val create : bool * bool -> Vle.t -> Vle.t -> Vle.t option -> Payload.t -> t
  val id : t -> Vle.t
  val prid : t -> Vle.t option
  val payload : t -> Payload.t
  val with_sn : t -> Vle.t -> t
  val with_id : t -> Vle.t -> t
end

module StreamData :
sig
  type body
  type t = body reliable marked block
  val create : bool * bool -> Vle.t -> Vle.t -> Payload.t -> t
  val id : t -> Vle.t
  val payload : t -> Payload.t
  val with_sn : t -> Vle.t -> t
  val with_id : t -> Vle.t -> t
end

module BatchedStreamData : 
sig
  type body
  type t = body reliable marked block
  val create : bool * bool -> Vle.t -> Vle.t  -> Payload.t list -> t
  val id : t -> Vle.t
  val payload : t -> Payload.t list
  val with_sn : t -> Vle.t -> t
  val with_id : t -> Vle.t -> t
end 


module Synch :
sig
  type body
  type t = body marked block
  val create : bool * bool -> Vle.t -> Vle.t option -> t
  val sn : t -> Vle.t
  val count : t -> Vle.t option
end

module AckNack :
sig
  type body
  type t = body marked block
  val create : Vle.t -> Vle.t option -> t
  val sn : t -> Vle.t
  val mask : t -> Vle.t option
end

module Migrate :
sig
  type body
  type t = body marked block
  val create : Vle.t -> Vle.t option -> Vle.t -> Vle.t -> t
  val ocid : t -> Vle.t
  val id : t -> Vle.t option
  val rch_last_sn : t -> Vle.t
  val bech_last_sn : t -> Vle.t
end

module Query :
sig
  type body
  type t = body marked block
  val create : Abuf.t -> Vle.t -> string -> string -> ZProperty.t list -> t
  val pid : t -> Abuf.t
  val qid : t -> Vle.t
  val resource : t -> string
  val predicate : t -> string
  val properties : t -> ZProperty.t list
end

module Reply :
sig
  type body
  type t = body marked block
  type source = | Storage | Eval
  val create : Abuf.t -> Vle.t -> source -> (Abuf.t * Vle.t * string * Payload.t) option ->t
  val qpid : t -> Abuf.t
  val qid : t -> Vle.t
  val final : t -> bool
  val value : t -> (Abuf.t * Vle.t * string * Payload.t) option
  val source : t -> source
  val stoid : t -> Abuf.t option
  val rsn : t -> Vle.t option
  val resource : t -> string option
  val payload : t -> Payload.t option
end

module Pull :
sig
  type body
  type t = body marked block
  val create : bool * bool -> Vle.t -> Vle.t -> Vle.t option -> t
  val sn : t -> Vle.t
  val id : t -> Vle.t
  val max_samples : t -> Vle.t option
  val final : t -> bool
  val sync : t -> bool
end

module PingPong :
sig
  type body
  type t = body marked block
  val create : ?pong:bool -> Vle.t -> t
  val is_pong : t -> bool
  val hash : t -> Vle.t
  val to_pong : t -> t
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

val markers : t -> marker list
val with_marker : t -> marker -> t
val with_markers : t -> marker list -> t
val remove_markers : t -> t

val to_string : t -> string
