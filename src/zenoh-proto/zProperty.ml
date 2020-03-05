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
open Ztypes

module PropertyId = struct
  let maxConduits = 2L
  let snLen = 4L
  let reliability = 6L
  let authData = 12L
  let destStorages = 16L
  let destEvals = 17L

  let nodeMask = 1L
  let storageDist = 3L

  let user = 0x50L
  let password = 0x51L
end

module T = Apero.KeyValueF.Make(Apero.Vle) (Abuf)
include T

let find_opt pid = List.find_opt (fun p -> Vle.to_int @@ key p = (Vle.to_int pid))

let on_value f p = 
  let buf = value p in 
  Abuf.mark_r_pos buf; 
  let res = f buf in
  Abuf.reset_r_pos buf;
  res

module NodeMask = struct 
  let make mask = make 
    PropertyId.nodeMask 
    (Abuf.create 32 |> fun buf -> Apero.fast_encode_vle mask buf; buf)
  
  let mask = on_value Apero.fast_decode_vle

  let find_opt = find_opt PropertyId.nodeMask
end 

module StorageDist = struct 
  let make dist = make 
    PropertyId.storageDist 
    (Abuf.create 32 |> fun buf -> Apero.fast_encode_vle dist buf; buf)
  
  let dist = on_value Apero.fast_decode_vle

  let find_opt = find_opt PropertyId.storageDist
end

module DestStorages = struct
  let make dest = make 
    PropertyId.destStorages 
    (match dest with 
      | Best_match -> Abuf.create 32 |> fun buf -> Apero.fast_encode_vle 0L buf; buf
      | Complete q -> Abuf.create 32 |> fun buf -> Apero.fast_encode_vle 1L buf; Apero.fast_encode_vle (Vle.of_int q) buf; buf
      | All        -> Abuf.create 32 |> fun buf -> Apero.fast_encode_vle 2L buf; buf
      | No         -> Abuf.create 32 |> fun buf -> Apero.fast_encode_vle 3L buf; buf)

  let dest = on_value @@ fun buf ->
    Apero.fast_decode_vle buf |> function
    | 1L -> Complete (Apero.fast_decode_vle buf |> Vle.to_int)
    | 2L -> All
    | 3L -> No
    | _ -> Best_match

  let find_opt = find_opt PropertyId.destStorages
end

module DestEvals = struct
  let make dest = make 
    PropertyId.destEvals 
    (match dest with 
      | Best_match -> Abuf.create 32 |> fun buf -> Apero.fast_encode_vle 0L buf; buf
      | Complete q -> Abuf.create 32 |> fun buf -> Apero.fast_encode_vle 1L buf; Apero.fast_encode_vle (Vle.of_int q) buf; buf
      | All        -> Abuf.create 32 |> fun buf -> Apero.fast_encode_vle 2L buf; buf
      | No         -> Abuf.create 32 |> fun buf -> Apero.fast_encode_vle 3L buf; buf)

  let dest = on_value @@ fun buf ->
    Apero.fast_decode_vle buf |> function
    | 1L -> Complete (Apero.fast_decode_vle buf |> Vle.to_int)
    | 2L -> All
    | 3L -> No
    | _ -> Best_match

  let find_opt = find_opt PropertyId.destEvals
end

module User = struct 
  let make name = make 
    PropertyId.user 
    (Abuf.create (String.length name) |> fun buf -> Abuf.write_bytes (Bytes.unsafe_of_string name) buf; buf)
  
  let name = on_value (fun buf -> Abuf.read_bytes (Abuf.readable_bytes buf) buf |> Bytes.unsafe_to_string) 

  let find_opt = find_opt PropertyId.user
end

module Password = struct 
  let make phrase = make 
    PropertyId.password 
    (Abuf.create (String.length phrase) |> fun buf -> Abuf.write_bytes (Bytes.unsafe_of_string phrase) buf; buf)
  
  let phrase = on_value (fun buf -> Abuf.read_bytes (Abuf.readable_bytes buf) buf |> Bytes.unsafe_to_string) 

  let find_opt = find_opt PropertyId.password
end