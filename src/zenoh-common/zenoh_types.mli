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
open Zenoh_common_errors

module Path = Apero.Path

module Selector : sig
  type t          
  val of_string : string -> t
  (** [of_string s] validate the format of the string [s] as a selector and returns a Selector if valid.
      If the validation fails, an [YException] is raised. *)
  val of_string_opt : string -> t option
  (** [of_string_opt s] validate the format of the string [s] as a selector and returns some Selector if valid.
      If the validation fails, None is returned. *)
  val to_string : t -> string
  (** [to_string s] return the Selector [s] as a string *)
  val of_path : ?predicate:string -> ?properties:string -> ?fragment:string -> Path.t -> t
  (** [of_path predicate properties fragment p] returns a Selector with its path equal to [p] with respectively
      the [predicate], [properties] and [fragment] optionally set *)
  val with_path : Path.t -> t -> t
  (** [with_path p s] returns a copy of the Selector [s] but with its path set to [p] *)

  val path : t -> string
  (** [path s] returns the path part of the Selector [s]. I.e. the part before any '?' character. *)
  val predicate : t -> string option
  (** [predicate s] returns the predicate part of the Selector [s].
      I.e. the substring after the first '?' character and before the first '(' or '#' character (if any).
      None is returned if their is no such substring. *)
  val properties : t -> string option
  (** [properties s] returns the properties part of the Selector [s].
      I.e. the substring enclosed between '(' and')' and which is after the first '?' character and before the fist '#' character (if any) ., or an empty string if no '?' is found.
      None is returned if their is no such substring. *)
  val fragment : t -> string option
  (** [fragment s] returns the fragment part of the Selector [s].
      I.e. the part after the first '#', or None if no '#' is found. *)
  val optional_part : t -> string
  (** [optional_part s] returns the part after the '?' character (i.e. predicate + properties + fragment) or an empty string if there is no '?' in the Selector *)

  val is_relative : t -> bool
  (**[is_relative s] return true if the path part ofthe Selector [s] is relative (i.e. it's first character is not '/') *)
  val add_prefix : prefix:Path.t -> t -> t
  (** [add_prefix prefix s] return a new Selector with path made of [prefix]/[path s].
      The predicate, properties and fragment parts of this new Selector are the same than [s]. *)
  val get_prefix : t -> Path.t
  (** [get_prefix s] return the longest Path (i.e. without any wildcard) that is a prefix in [s]
      The predicate, properties and fragment parts of [s] are ignored in this operation. *)

  val is_path_unique : t -> bool
  (** [is_path_unique s] returns true it the path part of Selector [s] doesn't contains any wildcard ('*'). *)
  val as_unique_path : t -> Path.t option
  (** [as_unique_path s] returns the path part of Selector [s] as Some Path if it doesn't contain any wildcard ('*').
      It returns None otherwise. *)

  val is_matching_path : Path.t -> t -> bool
  (** [is_matching_path p s] returns true if the selector [s] fully matches the path [p].
      Note that only the path part of the selector is considered for the matching (not the query and the fragment) *)
  val includes : subsel:t -> t -> bool
  (** [includes subsel s] returns true if the selector [s] includes the selector [subsel].
      I.e. if [subsel] matches a path p, [s] also matches p.  
      Note that only the path part of the selectors are considered for the inclusion (not the query and the fragment) *)
  val intersects : t -> t -> bool
  (** intersect s1 s2] returns true if the intersection of selector [s1] and [s2] is not empty.
      I.e. if it exists a path p that matches both expressions [s1] and [s2].
      Note that only the path part of the selectors are considered for the inclusion (not the query and the fragment) *)
  val remaining_after_match : Path.t -> t -> t option
  (** [remaining_after_match p s] checks if the Path [p] matches a substring prefixing the Selector [s].
      If there is such matching prefix in [s], a similar Selector than [s] is returned, but with this prefix removed
      from its path part. If there is no such matching, None is returned. *)
end [@@deriving show]

module Value : sig 
  (* TODO: change order of constants to have primitive types first. *)
  [%%cenum
  type encoding =
    | RAW          [@id  0x00]
    (* | CUSTOM       [@id  0x01] *)
    | STRING       [@id  0x02]
    | PROPERTIES   [@id  0x03]
    | JSON         [@id  0x04]
    | SQL          [@id  0x05]
    | INT          [@id  0x06]
    | FLOAT        [@id  0x07]
  [@@uint8_t]]

  type sql_row = string list
  type sql_column_names = string list

  type t =
    | RawValue of (string option * bytes)
    | StringValue of string
    | PropertiesValue of Apero.properties
    | JSonValue of string
    | SqlValue of (sql_row * sql_column_names option)
    | IntValue of Int64.t
    | FloatValue of float


  val of_string : string -> encoding -> (t, yerror) Apero.Result.t 
  (** [of_string s e] creates a new value from the string [s] and the encoding [e] *)
  val to_string : t -> string
  (** [to_string v] returns a string representation of the value [v] *)
  val encoding : t -> encoding
  (** [encoding v] returns the encoding of the value [v] *)
  val transcode : t -> encoding -> (t, yerror) Apero.Result.t
  (** [transcode v e] transcodes the value [v] to the encoding [e] and returns the resulting value *)

  val update : delta:t -> t -> (t, yerror) Apero.Result.t
  (** [update delta v] tries to update the value [v] with the partial value [delta].
      If [delta] has a different encoding than [v], it tries to {! transcode} [delta] into the same encoding thant [v] *)
end

module HLC = Zenoh_time.HLC
module Timestamp = Zenoh_time.Timestamp
module Time = Zenoh_time.Time

module TimedValue : sig
  type t = { time:Timestamp.t; value:Value.t }

  val update : delta:t -> t -> (t, yerror) Apero.Result.t
  val preceeds : first:t -> second:t -> bool
end

type change =
  | Put of TimedValue.t
  | Update of TimedValue.t
  | Remove of Timestamp.t
