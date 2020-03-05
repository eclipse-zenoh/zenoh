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

module VleMap = Map.Make(Vle)

module ResName = struct 
  type t  = | Path of PathExpr.t | ID of Vle.t

  let compare name1 name2 = 
    match name1 with 
    | ID id1 -> (match name2 with 
      | ID id2 -> Vle.compare id1 id2
      | Path _ -> 1)
    | Path uri1 -> (match name2 with 
      | ID _ -> -1
      | Path uri2 -> PathExpr.compare uri1 uri2)

  let name_match name1 name2 =
    match name1 with 
    | ID id1 -> (match name2 with 
      | ID id2 -> id1 = id2
      | Path _ -> false)
    | Path uri1 -> (match name2 with 
      | ID _ -> false
      | Path uri2 -> PathExpr.intersect uri1 uri2)

  let to_string = function 
    | Path uri -> PathExpr.to_string uri 
    | ID id -> Vle.to_string id
end 

module ResMap = Map.Make(ResName)