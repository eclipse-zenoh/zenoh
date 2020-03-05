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

(** NOTE: The current channel implementation uses only the default conduit,
    this should be extended to support arbitrary number of conduits. *)
module InChannel : sig
  type t
  val create : Vle.t -> t
  val rsn : t -> Vle.t
  val usn : t -> Vle.t
  val update_rsn : t -> Vle.t -> unit
  val update_usn : t -> Vle.t -> unit
  val sn_max : t -> Vle.t
end = struct
  type t = { sn_max: Vle.t; mutable rsn : Vle.t; mutable usn : Vle.t }

  let create sn_max = {sn_max; rsn = 0L ; usn = 0L }
  let rsn c = c.rsn
  let usn c = c.usn
  let update_rsn c sn = c.rsn <- sn
  let update_usn c sn = c.usn <- sn
  let sn_max c = c.sn_max
end

module OutChannel : sig
  type t
  val create : Vle.t -> t
  val rsn : t -> Vle.t
  val usn : t -> Vle.t
  val sn_max : t -> Vle.t
  val next_rsn : t -> Vle.t
  val next_usn : t -> Vle.t

end = struct
  type t = {sn_max: Vle.t; mutable rsn : Vle.t; mutable usn : Vle.t}
  let create sn_max  = { sn_max; rsn = 0L; usn = 0L }
  let rsn c = c.rsn
  let usn c = c.usn
  let sn_max c = c.sn_max

  let next_rsn c =
    let n = c.rsn in
    c.rsn <- Vle.add c.rsn 1L ; n

  let next_usn c =
    let n = c.usn in
    c.usn <- Vle.add c.usn 1L ; n

end
