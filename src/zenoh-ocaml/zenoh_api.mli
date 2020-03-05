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
open Zenoh_types

type zid = string
(** zenoh service identifier (unique in the system) *)
type feid = string
(** Frontend identifier (must be unique per zenoh service) *)
type beid = string
(** Backend identifier (must be unique per zenoh service) *)
type stid = string
(** Storage identifier (must be unique per zenoh service) *)
type sid = string
(** Session identifier (unique per zenoh service) *)
type subid = Zenoh_net.sub
(** Subscriber identifier (unique per zenoh service) *)


type listener_t = (Path.t * change) list -> unit Lwt.t
(** Callback function implemented by the listener *)

type eval_callback_t = Path.t -> properties -> Value.t Lwt.t
(** Callback function registered as an eval *)

type transcoding_fallback = Fail | Drop | Keep
(** Action to perform in [get] operation when the transcoding of a received [Value] 
    into the requested encoding fails.
    [Fail] means the [get] operation will return an error.
    [Drop] means the [get] operation will drop from the returned result the values which can't be transcoded.
    [Keep] means the [get] operation will return the values which can't be transcoded with their original encoding. *)


module RegisteredPath : sig
  type t

  val put : ?quorum:int -> Value.t -> t -> unit Lwt.t
  (** Similar to [Workspace.put] for a registered path. *)

  val update: ?quorum:int -> Value.t -> t -> unit Lwt.t
  (** Similar to [Workspace.update] for a registered path. *)

  val remove: ?quorum:int -> t  -> unit Lwt.t
  (** Similar to [Workspace.remove] for a registered path. *)
end

module Workspace : sig
  type t    

  val register_path : Path.t -> t -> RegisteredPath.t Lwt.t
  (** [register_path p] registers the Path [p] as a Publisher in Zenoh-net and returns a [RegisteredPath.t].
      A [RegisterdPath.t] can be used in case of successive put/update/remove on a same Path, to save bandwidth and get a better throughput.
      The [RegisterdPath] operations use the Zenoh-net.stream and Zenoh-net.lstream operations that avoid to send the Path as a string for each publication.
  *)

  val get : ?quorum:int -> ?encoding:Value.encoding -> ?fallback:transcoding_fallback -> Selector.t -> t -> (Path.t * Value.t) list Lwt.t
  (** [get quorum encoding fallback s ws] gets the set of tuples {e \{ <path,value> \} } available in zenoh for which {e path} 
    matches the {b selector} [s], where the selector [s] can be absolute or relative to the {e workspace} [ws]. 
      
    If a [quorum] is provided, then [get] will complete succesfully if and only if a number [quorum] of independent and complete storage set 
    exist. Complete storage means a storage that fully covers the selector (i.e. any path matching the selector is covered by the storage).
    This ensures that if there is a  {e \{ <path,value> \} } stored in zenoh for which the {e path} matches the selector [s], then
    there are at least [quorum] idependent copies of this element stored in zenoh. Of these [quorum] idependent copies, the one returned to the
    application is the most recent version.

    If no quorum is provided (notice this is the default behaviour) then the [get] will succeed even if there isn't a set
    of storages that fully covers the selector. I.e. storages that partially cover the selector will also reply.

    The [encoding]  allows an application to request values to be encoded in a specific format.
    See {! Zenoh_types.Value.encoding} for the available encodings.

    If no encoding is provided (this is the default behaviour) then zenoh will not try to perform any transcoding and will
    return matching values in the encoding in which they are stored.
    
    The [fallback] controls what happens for those values that cannot be transcoded into the desired encoding, the 
    available options are:
    - Fail: the [get] fails if some value cannot be transcoded.
    - Drop: values that cannot be transcoded are dropped.
    - Keep: values that cannot be transcoded are kept with their original encoding and left for the application to deal with.  *)

  val sget : ?quorum:int -> ?encoding:Value.encoding -> ?fallback:transcoding_fallback -> Selector.t -> t -> (Path.t * Value.t) Lwt_stream.t
  (** Similar to [get], but returning the set of tuples {e \{ <path,value> \} } as a Lwt_stream.t *)

  val put : ?quorum:int -> Path.t -> Value.t -> t -> unit Lwt.t
  (** [put quorum path value ws]  
    - causes the notification of all {e subscriptions} whose selector matches [path], and

    - stores the tuple {e <path,value> } on all {e storages} in zenoh whose {e selector} 
    matches [path]

    Notice that the [path] can be absolute or erelative to the workspace [ws].

    If a [quorum] is provided then the [put] will success only if and only if a number 
    [quorum] of independent storages exist that match [path]. If such a set exist, the 
    put operation will complete only ater the tuple {e <path,value> } 
    has been written on all these storages.

    If no quorum is provided, then no assumptions are made and the [put] always succeeds,
    even if there are currently no matching storage. In this case the only effect of this operation
    will be that of triggering matching subscriber, if any exist. *)

  val update: ?quorum:int -> Path.t -> Value.t -> t -> unit Lwt.t
  (** [update quorum path value ws] allows to {! put} a delta, thus avoiding to distribute the entire value. *) 

  val remove: ?quorum:int -> Path.t -> t  -> unit Lwt.t
  (** [remove quorum path ws] removes from all zenoh's storages the tuple having the given [path].
    [path] can be absolute or relative to the workspace [ws].
    If a [quorum] is provided, then the [remove] will complete only after having successfully removed the tuple 
    from [quorum] storages. *)

  val subscribe: ?listener:listener_t -> Selector.t -> t -> subid Lwt.t
  (** [subscribe listener selector ws] registers a subscription to tuples whose path matches the selector [s]. 
  
    A subscription identifier is returned.
    The [selector] can be absolute or relative to the workspace [ws]. If specified, 
    the [listener] callback will be called for each {! put} and {! update} on tuples whose
    path matches the subscription [selector] *)

  val unsubscribe: subid -> t -> unit Lwt.t
  (** [unsubscribe subid w] unregisters a previous subscription with the identifier [subid] *)

  val register_eval : Path.t -> eval_callback_t -> t -> unit Lwt.t
  (** [register_eval path eval ws] registers an evaluation function [eval] under the provided [path].
    The [path] can be absolute or relative to the workspace [ws]. *)

  val unregister_eval : Path.t -> t  -> unit Lwt.t
  (** [register_eval path ws] unregisters an previously registered evaluation function under the give [path].
    The [path] can be absolute or relative to the workspace [ws]. *)

end

module Admin : sig
  type t

  val add_backend : ?zid:zid -> beid -> properties -> t -> unit Lwt.t
  (** [add_backend zid beid p a] adds in the zenoh service with identifier [zid] (by default it will be the service this API is connected to)
      a backend with [beid] as identifier and [p] as properties. *)

  val get_backends : ?zid:zid -> t -> (beid * properties) list Lwt.t
  (** [get_backends zid a] returns the caracteristics (identifier and properties) of all the backends of the zenoh service with identifier [zid]
      (by default it will be the service this API is connected to). *)

  val get_backend : ?zid:zid -> beid -> t -> properties option Lwt.t
  (** [get_backend zid beid a] returns the properties of the backend with identifier [beid] of the zenoh service with identifier [zid]
      (by default it will be the service this API is connected to). *)

 val remove_backend : ?zid:zid -> beid -> t -> unit Lwt.t
 (** [remove_backend zid beid a] removes the backend with identifier [beid] from the zenoh service with identifier [zid]
      (by default it will be the service this API is connected to). *)

  val add_storage : ?zid:zid -> stid -> ?backend:beid -> properties -> t -> unit Lwt.t
  (** [add_storage zid stid beid p a] adds in the zenoh service with identifier [zid] (by default it will be the service this API is connected to)
      and using the backend with identifier [beid] a storage with [stid] as identifier and [p] as properties.
      Note that "selector" is a mandatory property.
      If [beid] is not specified, zenoh will automatically find a backend that can handle the specified properties. *)

  val get_storages : ?zid:zid -> ?backend:beid -> t -> (stid * properties) list Lwt.t
  (** [get_storages zid beid a] returns the caracteristics (identifier and properties) of all the storages of the zenoh service with identifier [zid]
      (by default it will be the service this API is connected to) and that are managed by the backend with identifier [beid].
      If [beid] is not specified, all the storages caracterisitics are returned, whatever their backend. *)

  val get_storage : ?zid:zid -> stid -> t -> properties option Lwt.t
  (** [get_storage zid stid a] returns the properties of the storage with identifier [stid] of the zenoh service with identifier [zid]
      (by default it will be the service this API is connected to). *)

  val remove_storage : ?zid:zid -> stid -> t -> unit Lwt.t
  (** [remove_storage zid stid a] removes the storage with identifier [stid] from the zenoh service with identifier [zid]
      (by default it will be the service this API is connected to). *)

end

module Infix : sig
  
  val (~//) : string -> Path.t
  (** [~// s] returns [s] as a Path if it's valid. Otherwise it raises an [Exception].
      Note that the Path's string is sanitized (i.e. it's trimmed meaningless '/' are removed).
      This operator is equal to { Path.of_string }. *)

  val (~/*) : string -> Selector.t
  (** [~/* s] returns [s] as a Selector if it's valid. Otherwise it raises an [Exception].
      Note that the expression is sanitized (i.e. it's trimmed meaningless '/' are removed)
      This operator is equal to { Selector.of_string }. *)

  val (~$) : string -> Value.t
  (** [~$ s] returns [s] as a Value with a { Value.String_Encoding } as encoding.
      This operator is equal to { Value.of_string [s] Value.String_Encoding }. *)

end


type t

val login : string -> properties -> t Lwt.t
(** [login l p] connects this API to the zenoh service using the locator [l] and login with the properties [p]. *)

val logout : t -> unit Lwt.t
(** [logout y] closes the connection of this API to the connected zenoh service *)

val get_id : t -> zid
(** [get_id y] returns the identifier of the zenoh service this API is connected to. *)

val workspace : Path.t -> t -> Workspace.t Lwt.t
(** [workspace p y] creates a new workspace with the working path [p].
    All non-absolute paths or selectors used with this workspace will be treated as relative to this working path *)

val admin : t -> Admin.t Lwt.t
(** [admin y] returns the Admin interface *)
